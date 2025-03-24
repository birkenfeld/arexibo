// Xibo player Rust implementation, (c) 2022-2024 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Handling resources such as media and layout files.

use std::collections::HashMap;
use std::{fs, io, io::Write, path::PathBuf, sync::Arc};
use anyhow::{ensure, Context, Result};
use md5::{Md5, Digest};
use serde::{Serialize, Deserialize};
use ureq::Agent;
use crate::{util, layout, layout::TRANSLATOR_VERSION, xmds};
use crate::config::CmsSettings;


/// An entry in the "required files" set.
#[derive(Debug)]
pub enum ReqFile {
    File {
        id: i64,
        typ: &'static str,
        size: u64,
        md5: Vec<u8>,
        http: bool,
        path: String,
        name: String,
    },
    Resource {
        id: i64,
        layoutid: i64,
        regionid: i64,
        mediaid: i64,
        updated: i64,
    },
}

impl ReqFile {
    pub fn description(&self) -> String {
        match self {
            ReqFile::File { typ, name, .. } => format!("{} {}", typ, name),
            ReqFile::Resource { mediaid, .. } => format!("resource {}", mediaid)
        }
    }

    pub fn inventory(&self) -> (&'static str, i64) {
        match self {
            ReqFile::File { id, typ, .. } => (typ, *id),
            ReqFile::Resource { id, .. } => ("resource", *id),
        }
    }
}

fn default_version() -> u32 { 0 }

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct LayoutInfo {
    pub id: i64,
    #[serde(deserialize_with = "util::de_hex", serialize_with = "util::ser_hex")]
    pub md5: Vec<u8>,
    pub size: (i32, i32),
    #[serde(default = "default_version")]
    pub translated_version: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MediaInfo {
    pub id: i64,
    pub size: u64,
    #[serde(deserialize_with = "util::de_hex", serialize_with = "util::ser_hex")]
    pub md5: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub id: i64,
    pub layoutid: i64,
    pub regionid: i64,
    pub updated: i64,
    pub duration: Option<f64>,
}

/// A resource in the local cache.
#[derive(Debug, Serialize, Deserialize)]
pub enum Resource {
    Layout(Arc<LayoutInfo>),
    Media(Arc<MediaInfo>),
    Resource(Arc<ResourceInfo>),
}


pub struct Cache {
    dir: PathBuf,
    agent: Agent,
    content: HashMap<String, Resource>,
}

impl Cache {
    pub fn new(cms: &CmsSettings, dir: PathBuf, clear: bool, no_verify: bool) -> Result<Self> {
        let mut content = HashMap::new();

        if !fs::metadata(&dir).map_or(false, |p| p.is_dir()) {
            // no directory? create it...
            fs::create_dir_all(&dir)?;
        } else if clear {
            // clear it?
            fs::remove_dir_all(&dir)?;
            fs::create_dir_all(&dir)?;
        }

        // check for a cached inventory JSON file
        if let Some(saved) = fs::File::open(dir.join("content.json"))
            .ok().and_then(|fp| serde_json::from_reader(fp).ok())
        {
            // ensure all mentioned files are present, remove missing entries
            content = saved;
            content.retain(|fname, _| dir.join(fname).is_file());

            // remove any layout descriptions if translated version is outdated
            content.retain(|_, res| match res {
                Resource::Layout(layout) => layout.translated_version == TRANSLATOR_VERSION,
                _ => true
            });
        }

        Ok(Self { dir, agent: cms.make_agent(no_verify)?, content })
    }

    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }

    pub fn has(&self, res: &ReqFile) -> bool {
        match *res {
            ReqFile::Resource { id, updated, .. } => {
                self.get_resource(id).map_or(false, |res| res.updated == updated)
            }
            ReqFile::File { ref name, ref md5, typ, id, .. } => {
                if typ == "layout" {
                    self.get_layout(id).map_or(false, |res| &res.md5 == md5)
                } else {
                    self.get_media(name).map_or(false, |res| &res.md5 == md5)
                }
            }
        }
    }

    pub fn download(&mut self, res: ReqFile, cms: &mut xmds::Cms) -> Result<()> {
        match res {
            ReqFile::Resource { id, layoutid, regionid, mediaid, updated } => {
                let data = cms.get_resource(layoutid, &regionid.to_string(),
                                            &mediaid.to_string())?;
                let fname = format!("{}.html", id);

                // TODO:
                // - process (replace [[ViewPort]], get DURATION)
                // - re-download after given updateInterval
                let duration = None;
                fs::write(self.dir.join(&fname), data)?;
                self.content.insert(fname, Resource::Resource(Arc::new(
                    ResourceInfo { id, layoutid, regionid, updated, duration }
                )));
                self.save()?;
            }
            ReqFile::File { id, typ, http, size, md5, path, name } => {
                let filename = self.dir.join(&name);
                if http {
                    match self.download_http(&path, &filename, &md5) {
                        Ok(_) => {},
                        Err(e) => {
                            log::warn!("failing download of {} over http, retrying \
                                        xmds: {:#}", name, e);
                            self.download_xmds(id, typ, size, cms, &filename, &md5)?
                        }
                    }
                } else {
                    self.download_xmds(id, typ, size, cms, &filename, &md5)?
                }

                if typ == "layout" {
                    // translate the layout into HTML
                    let xl = layout::Translator::new(
                        &self.dir.join(&name),
                        &self.dir.join(format!("{}.html", name))
                    )?;
                    let size = xl.translate()?;
                    self.content.insert(name, Resource::Layout(Arc::new(
                        LayoutInfo { id, md5, size, translated_version: TRANSLATOR_VERSION }
                    )));
                } else {
                    self.content.insert(name, Resource::Media(Arc::new(
                        MediaInfo { id, size, md5 }
                    )));
                }
                self.save()?;
            }
        }
        Ok(())
    }

    fn download_http(&mut self, path: &str, filename: &PathBuf,
                     md5: &[u8]) -> Result<()> {
        let body = self.agent.get(path).call()?.into_body();
        let file = io::BufWriter::new(fs::File::create(filename)?);
        let mut wrapper = HashingWriter::new(file);
        io::copy(&mut body.into_reader(), &mut wrapper)?;
        ensure!(wrapper.hash() == md5, "md5 mismatch");
        Ok(())
    }

    fn download_xmds(&mut self, id: i64, typ: &str, size: u64, cms: &mut xmds::Cms,
                     filename: &PathBuf, md5: &[u8]) -> Result<()> {
        const CHUNK_SIZE: u64 = 1024 * 1024;
        let mut got_size = 0;
        let file = io::BufWriter::new(fs::File::create(filename)?);
        let mut wrapper = HashingWriter::new(file);
        while got_size < size {
            let next_size = (size - got_size).min(CHUNK_SIZE);
            let chunk = cms.get_file_data(id, typ, got_size, next_size)?;
            got_size += chunk.len() as u64;
            wrapper.write_all(&chunk)?;
        }
        ensure!(wrapper.hash() == md5, "md5 mismatch");
        Ok(())
    }

    pub fn get_layout(&self, id: i64) -> Option<Arc<LayoutInfo>> {
        self.content.get(&format!("{}.xlf", id)).and_then(|entry| match entry {
            Resource::Layout(layout) => Some(layout.clone()),
            _ => None
        })
    }

    fn get_media(&self, name: &str) -> Option<Arc<MediaInfo>> {
        self.content.get(name).and_then(|entry| match entry {
            Resource::Media(media) => Some(media.clone()),
            _ => None
        })
    }

    fn get_resource(&self, id: i64) -> Option<Arc<ResourceInfo>> {
        self.content.get(&format!("{}.html", id)).and_then(|entry| match entry {
            Resource::Resource(res) => Some(res.clone()),
            _ => None
        })
    }

    fn save(&self) -> Result<()> {
        let fp = fs::File::create(self.dir.join("content.json")).context("writing cache content")?;
        serde_json::to_writer_pretty(fp, &self.content).context("serializing cache content")?;
        Ok(())
    }
}


pub struct HashingWriter<W> {
    writer: W,
    hasher: Md5,
}

impl<W> HashingWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer, hasher: md5::Md5::new() }
    }

    pub fn hash(self) -> Vec<u8> {
        self.hasher.finalize().as_slice().to_vec()
    }
}

impl<W> Write for HashingWriter<W> where W: Write {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = self.writer.write(buf)?;
        self.hasher.update(buf);
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}
