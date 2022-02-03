// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Send SOAP requests to the XMDS endpoint.

mod soap {
    #![allow(non_snake_case)]
    include!(concat!(env!("OUT_DIR"), "/xmds_soap.rs"));
}

use anyhow::{bail, Context, Result};
use elementtree::Element;
use crate::config::{CmsSettings, PlayerSettings};
use crate::util::{Base64Field, ElementExt};
use crate::res::{FileType, Resource};
use crate::sched::Schedule;

pub struct Cms {
    service: soap::Service,
    channel: String,
    cms_key: String,
    hw_key: String,
    pub_key: String,
}

impl Cms {
    pub fn new(settings: &CmsSettings, pub_key: String) -> Self {
        Self {
            service: soap::Service::new(format!("{}/xmds.php?v=5", settings.address)),
            channel: settings.xmr_channel(),
            cms_key: settings.key.to_owned(),
            hw_key: settings.display_id.to_owned(),
            pub_key
        }
    }

    pub fn register_display(&mut self) -> Result<Option<PlayerSettings>> {
        let xml = self.service.RegisterDisplay(
            soap::RegisterDisplayRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                displayName: "Rust Display",
                clientType: "linux",
                clientVersion: &clap::crate_version!(),
                clientCode: 0,
                operatingSystem: "linux",
                macAddress: "00:00:00:00:00:00",  // TODO
                xmrChannel: &self.channel,
                xmrPubKey: &self.pub_key,
            }
        ).context("registering display")?.ActivationMessage;

        let tree = Element::from_reader(&mut xml.as_bytes()).context("parsing activation message")?;
        let code = tree.get_attr("code").context("no result code in activation")?;
        if code != "READY" {
            Ok(None)
        } else {
            Ok(Some(PlayerSettings {
                xmr_network_address: tree.parse_child("xmrNetworkAddress")?,
                log_level: tree.parse_child("logLevel")?,
                display_name: tree.parse_child("displayName")?,
                stats_enabled: tree.parse_child::<i32>("statsEnabled")? != 0,
                prevent_sleep: tree.parse_child::<i32>("preventSleep")? != 0,
                collect_interval: tree.parse_child("collectInterval")?,
                screenshot_interval: tree.parse_child("screenShotRequestInterval")?,
                embedded_server_port: tree.parse_child("embeddedServerPort")?,
                size_x: tree.parse_child("sizeX")?,
                size_y: tree.parse_child("sizeY")?,
                position_x: tree.parse_child("offsetX")?,
                position_y: tree.parse_child("offsetY")?,
            }))
        }
    }

    pub fn required_files(&mut self) -> Result<Vec<Resource>> {
        let xml = self.service.RequiredFiles(
            soap::RequiredFilesRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
            }
        ).context("getting required files")?.RequiredFilesXml;

        let tree = Element::from_reader(&mut xml.as_bytes()).context("parsing required files")?;
        let mut res = vec![];
        for file in tree.find_all("file") {
            let typ = file.get_attr("type").context("missing file type")?;
            if typ == "media" || typ == "layout" {
                let http = file.get_attr("download").context("missing download")? == "http";
                let (path, name) = if http {
                    (file.parse_attr("path")?, file.parse_attr("saveAs")?)
                } else {
                    let mut path= file.parse_attr::<String>("path")?;
                    if typ == "layout" {
                        path.push_str(".xlf");
                    }
                    ("".into(), path)
                };
                res.push(Resource::File {
                    id: file.parse_attr("id")?,
                    typ: if typ == "media" { FileType::Media } else { FileType::Layout },
                    size: file.parse_attr("size")?,
                    md5: hex::decode(&file.parse_attr::<String>("md5")?)?,
                    path, name, http,
                });
            } else if typ == "resource" {
                res.push(Resource::Resource {
                    id: file.parse_attr("id")?,
                    layoutid: file.parse_attr("layoutid")?,
                    regionid: file.parse_attr("regionid")?,
                    mediaid: file.parse_attr("mediaid")?,
                    updated: file.parse_attr("updated")?,
                })
            } else {
                continue;
            }
        }
        Ok(res)
    }

    pub fn get_schedule(&mut self) -> Result<Schedule> {
        let xml = self.service.Schedule(
            soap::ScheduleRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
            }
        ).context("getting schedule")?.ScheduleXml;

        let tree = Element::from_reader(&mut xml.as_bytes()).context("parsing schedule")?;
        Schedule::parse(tree)
    }

    pub fn get_file_data(&mut self, file: i64, ftype: FileType, offset: u64, size: u64) -> Result<Vec<u8>> {
        Ok(self.service.GetFile(
            soap::GetFileRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                fileId: file,
                fileType: ftype.as_str(),
                chunkOffset: offset as f64,
                chuckSize: size as f64,
            }
        ).context("getting file data")?.file.0)
    }

    pub fn get_resource(&mut self, layout: i64, region: &str, media: &str) -> Result<String> {
        Ok(self.service.GetResource(
            soap::GetResourceRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                layoutId: layout,
                regionId: region,
                mediaId: media,
            }
        ).context("getting resource")?.resource)
    }

    pub fn blacklist(&mut self, media: i64, mtype: &str, reason: &str) -> Result<()> {
        let success = self.service.BlackList(
            soap::BlackListRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                mediaId: media,
                r#type: mtype,
                reason,
            }
        ).context("blacklisting media")?.success;
        if !success { bail!("blacklisting not successful"); } else { Ok(()) }
    }

    pub fn submit_media_inventory(&mut self, inv: Vec<(&'static str, i64, bool)>) -> Result<()> {
        let mut files = Element::new("files");
        for (typ, id, complete) in inv {
            let mut file = Element::new("file");
            file.set_attr("type", typ);
            file.set_attr("id", &id.to_string());
            file.set_attr("complete", if complete { "1" } else { "0" });
            files.append_child(file);
        }

        let inv_xml = format!("<![CDATA[ {} ]]>", files.to_string()?);
        let success = self.service.MediaInventory(
            soap::MediaInventoryRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                mediaInventory: &inv_xml,
            }
        ).context("submitting media inventory")?.success;
        if !success { bail!("submitting inventory not successful"); } else { Ok(()) }
    }

    pub fn submit_log(&mut self, log_xml: &str) -> Result<()> {
        let success = self.service.SubmitLog(
            soap::SubmitLogRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                logXml: log_xml
            }
        ).context("submitting logs")?.success;
        if !success { bail!("submitting logs not successful"); } else { Ok(()) }
    }

    pub fn submit_stats(&mut self, stat_xml: &str) -> Result<()> {
        let success = self.service.SubmitStats(
            soap::SubmitStatsRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                statXml: stat_xml
            }
        ).context("submitting stats")?.success;
        if !success { bail!("submitting stats not successful"); } else { Ok(()) }
    }

    pub fn submit_screenshot(&mut self, shot: Vec<u8>) -> Result<()> {
        let success = self.service.SubmitScreenShot(
            soap::SubmitScreenShotRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                screenShot: Base64Field(shot),
            }
        ).context("submitting screenshot")?.success;
        if !success { bail!("submitting screenshot not successful"); } else { Ok(()) }
    }

    pub fn notify_status(&mut self, status: &str) -> Result<()> {
        let success = self.service.NotifyStatus(
            soap::NotifyStatusRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                status,  // TODO: enum
            }
        ).context("notifying status")?.success;
        if !success { bail!("notify status not successful"); } else { Ok(()) }
    }
}