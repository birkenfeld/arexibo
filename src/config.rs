// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Definitions for the player configuration.

use std::path::Path;
use anyhow::{Context, Result};
use md5::{Md5, Digest};
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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
    pub position_x: i32,
    #[serde(default)]
    pub position_y: i32,
}

impl PlayerSettings {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read(path.as_ref()).context("reading player settings")?;
        toml::from_slice(&content).context("deserializing player settings")
    }

    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let content = toml::to_string_pretty(self).context("serializing player settings")?;
        std::fs::write(path.as_ref(), content.as_bytes()).context("writing player settings")
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
    // TODO: Proxy
}

impl CmsSettings {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read(path.as_ref()).context("reading cms settings")?;
        toml::from_slice(&content).context("deserializing cms settings")
    }

    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let content = toml::to_string_pretty(self).context("serializing cms settings")?;
        std::fs::write(path.as_ref(), content.as_bytes()).context("writing cms settings")
    }

    pub fn xmr_channel(&self) -> String {
        let to_hash = format!("{}{}{}", self.address, self.key, self.display_id);
        hex::encode(&Md5::digest(&to_hash))
    }
}