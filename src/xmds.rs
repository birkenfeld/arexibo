// Xibo player Rust implementation, (c) 2022-2024 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Send SOAP requests to the XMDS endpoint.

/// Includes the SOAP glue code generated by build.rs.
mod soap {
    #![allow(non_snake_case)]
    include!(concat!(env!("OUT_DIR"), "/xmds_soap.rs"));
}

use std::{collections::HashMap, fs, path::PathBuf};
use anyhow::{ensure, Context, Result};
use elementtree::Element;
use serde::Serialize;
use crate::config::{CmsSettings, PlayerSettings};
use crate::command::Command;
use crate::util::{TIME_FMT, Base64Field, ElementExt, retrieve_mac, get_display_name};
use crate::resource::ReqFile;
use crate::schedule::Schedule;
use crate::logger::LogEntry;

/// Proxy for the XMDS calls to the CMS.
pub struct Cms {
    service: soap::Service,
    xml_dir: PathBuf,
    display_name: String,
    mac_addr: String,
    channel: String,
    cms_key: String,
    hw_key: String,
    pub_key: String,
}

impl Cms {
    pub fn new(cms: &CmsSettings, pub_key: String, no_verify: bool, xml_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            service: soap::Service::new(format!("{}/xmds.php?v=5", cms.address),
                                        cms.make_agent(no_verify)?),
            display_name: cms.display_name.as_ref().map_or_else(get_display_name,
                                                                |name| name.to_owned()),
            mac_addr: retrieve_mac().unwrap_or_else(|| "00:00:00:00:00:00".into()),
            channel: cms.xmr_channel(),
            cms_key: cms.key.to_owned(),
            hw_key: cms.display_id.to_owned(),
            pub_key,
            xml_dir,
        })
    }

    pub fn register_display(&mut self) -> Result<Option<PlayerSettings>> {
        let xml = self.service.RegisterDisplay(
            soap::RegisterDisplayRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                displayName: &self.display_name,
                clientType: "linux",
                clientVersion: clap::crate_version!(),
                clientCode: 0,
                operatingSystem: "linux",
                macAddress: &self.mac_addr,
                xmrChannel: &self.channel,
                xmrPubKey: &self.pub_key,
            }
        ).context("registering display")?.ActivationMessage;
        let _ = fs::write(self.xml_dir.join("register.xml"), &xml);

        let tree = Element::from_reader(&mut xml.as_bytes()).context("parsing activation message")?;
        let code = tree.get_attr("code").context("no result code in activation")?;
        if code != "READY" {
            Ok(None)
        } else {
            let mut commands = HashMap::new();
            for cmds in tree.find_all("commands") {
                for el in cmds.children() {
                    commands.insert(el.tag().name().into(),
                                    Command {
                                        command: el.parse_child("commandString")?,
                                        validate: el.parse_child("validationString")?,
                                        alerts: el.parse_child("createAlertOn")?,
                                    });
                }
            }

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
                pos_x: tree.parse_child("offsetX")?,
                pos_y: tree.parse_child("offsetY")?,
                commands,
            }))
        }
    }

    pub fn required_files(&mut self) -> Result<(Vec<ReqFile>, Vec<String>)> {
        let xml = self.service.RequiredFiles(
            soap::RequiredFilesRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
            }
        ).context("getting required files")?.RequiredFilesXml;
        let _ = fs::write(self.xml_dir.join("required.xml"), &xml);

        let tree = Element::from_reader(&mut xml.as_bytes()).context("parsing required files")?;
        let mut res = vec![];
        let mut purge = vec![];
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
                res.push(ReqFile::File {
                    id: file.parse_attr("id")?,
                    // match seems like a no-op but maps to a &'static str
                    typ: match typ { "media" => "media", "layout" => "layout",
                                      _ => unreachable!() },
                    size: file.parse_attr("size")?,
                    md5: hex::decode(&file.parse_attr::<String>("md5")?)?,
                    code: file.get_attr("code").map(Into::into),
                    path, name, http,
                });
            } else if typ == "resource" {
                res.push(ReqFile::Resource {
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
        for subtree in tree.find_all("purge") {
            for item in subtree.find_all("item") {
                if let Some(name) = item.get_attr("storedAs") {
                    purge.push(name.into());
                }
            }
        }
        Ok((res, purge))
    }

    pub fn get_schedule(&mut self) -> Result<Schedule> {
        let xml = self.service.Schedule(
            soap::ScheduleRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
            }
        ).context("getting schedule")?.ScheduleXml;
        let _ = fs::write(self.xml_dir.join("schedule.xml"), &xml);

        let tree = Element::from_reader(&mut xml.as_bytes()).context("parsing schedule")?;
        Schedule::parse(tree)
    }

    pub fn get_file_data(&mut self, file: i64, ftype: &str, offset: u64, size: u64) -> Result<Vec<u8>> {
        Ok(self.service.GetFile(
            soap::GetFileRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                fileId: file,
                fileType: ftype,
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
        let res = self.service.BlackList(
            soap::BlackListRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                mediaId: media,
                r#type: mtype,
                reason,
            }
        ).context("blacklisting media")?;
        ensure!(res.success, "blacklisting not successful");
        Ok(())
    }

    pub fn submit_media_inventory(&mut self, inv: Vec<((&'static str, i64), bool)>) -> Result<()> {
        let mut files = Element::new("files");
        for ((typ, id), complete) in inv {
            let mut file = Element::new("file");
            file.set_attr("type", typ);
            file.set_attr("id", &id.to_string());
            file.set_attr("complete", if complete { "1" } else { "0" });
            files.append_child(file);
        }

        let inv_xml = format!("<![CDATA[{}]]>", files.to_string()?);
        let res = self.service.MediaInventory(
            soap::MediaInventoryRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                mediaInventory: &inv_xml,
            }
        ).context("submitting media inventory")?;
        ensure!(res.success, "submitting inventory not successful");
        Ok(())
    }

    pub fn submit_log(&mut self, entries: &[LogEntry]) -> Result<()> {
        let mut logs = Element::new("logs");
        for entry in entries {
            let mut log = Element::new("log");
            log.set_attr("date", entry.date.format(&TIME_FMT).expect("time fmt"));
            log.set_attr("category", entry.category);
            log.append_child(Element::new("message")).set_text(&entry.message);
            logs.append_child(log);
        }

        let log_xml = format!("<![CDATA[{}]]>", logs.to_string()?);
        let res = self.service.SubmitLog(
            soap::SubmitLogRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                logXml: &log_xml
            }
        ).context("submitting logs")?;
        ensure!(res.success, "submitting logs not successful");
        Ok(())
    }

    pub fn submit_stats(&mut self, stat_xml: &str) -> Result<()> {
        let res = self.service.SubmitStats(
            soap::SubmitStatsRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                statXml: stat_xml
            }
        ).context("submitting stats")?;
        ensure!(res.success, "submitting stats not successful");
        Ok(())
    }

    pub fn submit_screenshot(&mut self, shot: Vec<u8>) -> Result<()> {
        let res = self.service.SubmitScreenShot(
            soap::SubmitScreenShotRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                screenShot: Base64Field(shot),
            }
        ).context("submitting screenshot")?;
        ensure!(res.success, "submitting screenshot not successful");
        Ok(())
    }

    pub fn notify_command_success(&mut self, result: bool) -> Result<()> {
        self.notify_status_raw(format!("{{\"lastCommandSuccess\": {}}}", result))
    }

    pub fn notify_status(&mut self, status: Status<'_>) -> Result<()> {
        self.notify_status_raw(serde_json::to_string(&status)?)
    }

    fn notify_status_raw(&mut self, status: String) -> Result<()> {
        let res = self.service.NotifyStatus(
            soap::NotifyStatusRequest {
                serverKey: &self.cms_key,
                hardwareKey: &self.hw_key,
                status: &status,
            }
        ).context("notifying status")?;
        ensure!(res.success, "status notification not successful");
        Ok(())
    }
}

#[allow(non_snake_case)]
#[derive(Serialize)]
pub struct Status<'s> {
    pub currentLayoutId: i64,
    pub availableSpace: u64,
    pub totalSpace: u64,
    pub lastCommandSuccess: bool,
    pub deviceName: &'s str,
    pub timeZone: &'s str,
    // pub latitude: f64,
    // pub longitude: f64,
}
