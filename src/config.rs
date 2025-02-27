// Xibo player Rust implementation, (c) 2022-2024 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Definitions for the player configuration.

use std::{fs::File, path::Path, time::Duration};
use anyhow::{Context, Result};
use md5::{Md5, Digest};
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PlayerSettings {
    #[serde(default = "default_collect_interval")]
    pub collect_interval: u64,
    #[serde(default)]
    pub stats_enabled: bool,
    #[serde(default)]
    pub xmr_network_address: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub screenshot_interval: u64,
    #[serde(default = "default_embedded_server_port")]
    pub embedded_server_port: u16,
    #[serde(default)]
    pub prevent_sleep: bool,
    #[serde(default = "default_display_name")]
    pub display_name: String,
    #[serde(default)]
    pub size_x: i32,
    #[serde(default)]
    pub size_y: i32,
    #[serde(default)]
    pub pos_x: i32,
    #[serde(default)]
    pub pos_y: i32,
}

impl PlayerSettings {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        serde_json::from_reader(File::open(path.as_ref())?)
            .context("deserializing player settings")
    }

    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        serde_json::to_writer_pretty(File::create(path.as_ref())?, self)
            .context("serializing player settings")
    }
}

fn default_collect_interval() -> u64 { 900 }
fn default_log_level() -> String { "debug".into() }
fn default_embedded_server_port() -> u16 { 9696 }
fn default_display_name() -> String { "Xibo".into() }

#[derive(Debug, Serialize, Deserialize)]
pub struct CmsSettings {
    pub address: String,
    pub key: String,
    pub display_id: String,
    pub display_name: Option<String>,
    pub proxy: Option<String>,
}

impl CmsSettings {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        serde_json::from_reader(File::open(path.as_ref())?)
            .context("deserializing player settings")
    }

    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        serde_json::to_writer_pretty(File::create(path.as_ref())?, self)
            .context("serializing player settings")
    }

    pub fn xmr_channel(&self) -> String {
        let to_hash = format!("{}{}{}", self.address, self.key, self.display_id);
        hex::encode(Md5::digest(to_hash))
    }

    pub fn make_agent(&self, no_verify: bool) -> Result<ureq::Agent> {
        let tls_config = ureq::tls::TlsConfig::builder()
            .disable_verification(no_verify)
            .build();
        let proxy = if let Some(proxy) = &self.proxy {
            Some(ureq::Proxy::new(proxy)?)
        } else {
            None
        };
        Ok(ureq::config::Config::builder()
            .timeout_connect(Some(Duration::from_secs(3)))
            .tls_config(tls_config)
            .proxy(proxy)
            .build().into())
    }
}
