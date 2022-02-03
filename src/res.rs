// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Handling resources such as media and layout files.

use std::{sync::Arc, fs, io::Read, io::Seek, thread};
use std::path::PathBuf;
use anyhow::{anyhow, bail, Result};
use md5::{Md5, Digest};
use tiny_http::{Request, Response, ResponseBox, Header, StatusCode};
use ureq::Agent;
use crate::xmds;
// use crate::layout;

#[derive(Debug, Clone, Copy)]
pub enum FileType {
    Media,
    Layout,
}

impl FileType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileType::Media => "media",
            FileType::Layout => "layout",
        }
    }
}

#[derive(Debug)]
pub enum Resource {
    File {
        id: i64,
        typ: FileType,
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

impl Resource {
    pub fn description(&self) -> String {
        match self {
            Resource::File { typ, name, .. } => format!("{} {}", typ.as_str(), name),
            Resource::Resource { mediaid, .. } => format!("resource {}", mediaid)
        }
    }

    pub fn inventory(&self, complete: bool) -> (&'static str, i64, bool) {
        match self {
            Resource::File { id, typ, .. } => (typ.as_str(), *id, complete),
            Resource::Resource { mediaid, .. } => ("resource", *mediaid, complete),
        }
    }
}


pub struct Cache {
    dir: PathBuf,
    agent: Agent,
}

impl Cache {
    pub fn new(dir: PathBuf) -> Result<Self> {
        if !fs::metadata(&dir).map_or(false, |p| p.is_dir()) {
            fs::create_dir_all(&dir)?;
        }
        let agent = Agent::new();
        Ok(Self { dir, agent })
    }

    pub fn has(&self, res: &Resource) -> bool {
        match res {
            &Resource::Resource { mediaid, .. } => {
                fs::metadata(self.dir.join(format!("{}.html", mediaid))).map_or(false, |p| p.is_file())
            }
            &Resource::File { ref name, .. } => {
                fs::metadata(self.dir.join(name)).map_or(false, |p| p.is_file())
            }
        }
    }

    pub fn download(&mut self, res: &Resource, cms: &mut xmds::Cms) -> Result<()> {
        match res {
            &Resource::Resource { layoutid, regionid, mediaid, .. } => {
                let data = cms.get_resource(layoutid, &regionid.to_string(),
                                            &mediaid.to_string())?;
                let fname = format!("{}.html", mediaid);
                fs::write(self.dir.join(&fname), &data)?;
            }
            &Resource::File { id, typ, http, size, ref md5, ref path, ref name } => {
                let data = if http {
                    match self.download_http(path) {
                        Ok(data) => data,
                        Err(e) => {
                            log::warn!("failing download of {} over http, retrying \
                                        xmds: {:#}", name, e);
                            self.download_xmds(id, typ, size, cms)?
                        }
                    }
                } else {
                    self.download_xmds(id, typ, size, cms)?
                };
                if Md5::digest(&data).as_slice() != md5 {
                    bail!("md5 mismatch");
                }
                fs::write(self.dir.join(name), data)?;
            }
        }
        Ok(())
    }

    fn download_http(&mut self, path: &str) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        self.agent.get(path).call()?.into_reader().read_to_end(&mut data)?;
        Ok(data)
    }

    fn download_xmds(&mut self, id: i64, typ: FileType, size: u64, cms: &mut xmds::Cms) -> Result<Vec<u8>> {
        const CHUNK_SIZE: u64 = 1024 * 1024;
        let mut got_size = 0;
        let mut result = Vec::new();
        while got_size < size {
            let next_size = (size - got_size).min(CHUNK_SIZE);
            let chunk = cms.get_file_data(id, typ, got_size, next_size)?;
            got_size += chunk.len() as u64;
            result.extend(chunk);
        }
        Ok(result)
    }
}


/// Internal webserver that is used to serve layouts and media to the webview.
pub struct Server {
    dir: PathBuf,
    server: tiny_http::Server,
}

impl Server {
    pub fn new(dir: PathBuf, port: u16) -> Result<Self> {
        let server = tiny_http::Server::http(("127.0.0.1", port))
            .map_err(|e| anyhow!(e))?;
        Ok(Self { dir, server })
    }

    pub fn start_pool(self) {
        let server = Arc::new(self.server);
        for _ in 0..4 {
            let server = server.clone();
            let dir = self.dir.clone();
            thread::spawn(move || {
                loop {
                    let req = server.recv().unwrap();
                    match Self::serve(&dir, &req) {
                        Ok(resp) => {  let _ = req.respond(resp); }
                        Err(e) => {
                            log::warn!("processing HTTP req {}: {:#}", req.url(), e);
                            let _ = req.respond(Response::empty(500));
                        }
                    }
                }
            });
        }
    }

    fn serve(dir: &PathBuf, req: &Request) -> Result<ResponseBox> {
        log::debug!("HTTP request: {}", req.url());
        Ok(match req.url() {
            "/splash.jpg" => Response::from_data(SPLASH_JPG).boxed(),
            "/splash.html" => Response::from_data(SPLASH_HTML).boxed(),
            path if path.starts_with("/layout?") => {
                let id: i64 = path[8..].parse()?;
                let htmlpath = dir.join(format!("{}.xlf.html", id));
                if !htmlpath.is_file() {
                    let xlfpath = dir.join(format!("{}.xlf", id));
                    if xlfpath.is_file() {
                        log::info!("requested layout {}, needs processing", id);
                        // layout::translate(&xlfpath, &htmlpath)?;
                        //     .context("translating layout")?;
                        Response::from_file(fs::File::open(htmlpath)?).boxed()
                    } else {
                        Response::empty(404).boxed()
                    }
                } else {
                    Response::from_file(fs::File::open(htmlpath)?).boxed()
                }
            },
            path => {
                let path = dir.join(&path[1..]);
                if !path.is_file() {
                    return Ok(Response::empty(404).boxed());
                }
                let mut fp = fs::File::open(&path)?;

                // implement HTTP Range query for gstreamer
                for h in req.headers() {
                    if h.field.equiv("Range") {
                        let total_size = fp.metadata()?.len();
                        let requested = h.value.to_string();
                        let mut parts = requested.split(&['=', '-'][..]);
                        let (from, to) = match (parts.next(), parts.next(), parts.next()) {
                            (Some("bytes"), Some(from), Some(to)) => {
                                (from.parse().unwrap_or(0), to.parse().unwrap_or(total_size - 1))
                            }
                            _ => bail!("invalid Range header")
                        };
                        if ! (from <= to && to < total_size) {
                            bail!("invalid Range from/to")
                        }
                        let size = to - from + 1;
                        fp.seek(std::io::SeekFrom::Start(from))?;
                        let stream = fp.take(size);

                        let range = format!("bytes {}-{}/{}", from, to, total_size);
                        return Ok(Response::new(
                            StatusCode(206),
                            vec![
                                Header::from_bytes(&b"Content-Range"[..],
                                                   range.as_bytes()).unwrap(),
                            ],
                            stream,
                            Some(size as usize),
                            None
                        ).boxed());
                    }
                }

                let ctype = match path.extension().and_then(|e| e.to_str()) {
                    Some("html") => "text/html",
                    Some("js") => "text/javascript",
                    Some("ttf") | Some("otf") => "application/font-sfnt",
                    Some("png") => "image/png",
                    Some("jpg") | Some("jpeg") => "image/jpeg",
                    Some("pdf") => "application/pdf",
                    Some("mp4") => "video/mp4",
                    Some("avi") => "video/avi",
                    Some("ogv") => "video/ogg",
                    Some("webm") => "video/webm",
                    _ => "",
                };

                Response::from_file(fp)
                    // for gstreamer, need a response with Content-Length => no chunked
                    .with_chunked_threshold(usize::MAX)
                    .with_header(Header::from_bytes(&b"Content-Type"[..], ctype.as_bytes()).unwrap())
                    .boxed()
            }
        })
    }
}

const SPLASH_HTML: &[u8] = br#"<!doctype html>
<html>
<body style="margin: 0">
<img style="width: 100%; height: 100%" src="splash.jpg">
</body>
</html>
"#;

const SPLASH_JPG: &[u8] = include_bytes!("../assets/splash.jpg");


// TODO:
// - central + persisted storage of media info, layout info
// - move server to own module
