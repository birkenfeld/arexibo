// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Various utilities.

use std::{fs, fmt, str::FromStr, time::Duration};
use anyhow::{Context, Result};
use dbus::blocking::{Connection};
use md5::{Md5, Digest};
use nix::unistd::gethostname;
use serde::{Deserialize, Deserializer, Serializer, de::Error};


/// Wrapper to send binary data as Base64 over SOAP.
#[derive(Debug)]
pub struct Base64Field(pub Vec<u8>);

impl fmt::Display for Base64Field {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", base64::encode(&self.0))
    }
}

impl FromStr for Base64Field {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(Base64Field(base64::decode(s)?))
    }
}


/// Helpers for parsing XML
pub trait ElementExt {
    fn def_attr<'a>(&'a self, attr: &'a str, def: &'a str) -> &'a str;
    fn parse_attr<T: FromStr>(&self, attr: &str) -> Result<T>
        where T::Err: std::error::Error + Sync + Send + 'static;
    fn parse_child<T: FromStr>(&self, child: &str) -> Result<T>
        where T::Err: std::error::Error + Sync + Send + 'static;
}

impl ElementExt for elementtree::Element {
    fn def_attr<'a>(&'a self, attr: &'a str, def: &'a str) -> &'a str {
        self.get_attr(attr).unwrap_or(def)
    }

    fn parse_attr<T: FromStr>(&self, attr: &str) -> Result<T>
        where T::Err: std::error::Error+Sync+Send+'static
    {
        self.get_attr(attr).with_context(|| format!("missing {}", attr))?
                           .parse().with_context(|| format!("invalid {}", attr))
    }

    fn parse_child<T: FromStr>(&self, child: &str) -> Result<T>
        where T::Err: std::error::Error+Sync+Send+'static
    {
        self.find(child).with_context(|| format!("missing {}", child))?
                        .text()
                        .parse().with_context(|| format!("invalid {}", child))
    }
}

/// Minimal console logger
pub struct ConsoleLog;

impl log::Log for ConsoleLog {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        let path = record.module_path().unwrap_or("");
        if !path.starts_with("arexibo") {
            return;
        }
        println!("{:5}: [{}] {}", record.level(), path, record.args());
    }
    fn flush(&self) {}
}


pub fn percent_decode(s: &str) -> String {
    let mut res = String::new();
    let mut iter = s.char_indices();
    while let Some((i, ch)) = iter.next() {
        match ch {
            '%' => {
                let codepoint = s.get(i+1..i+3)
                                 .and_then(|s| u8::from_str_radix(s, 16).ok());
                if let Some(hex) = codepoint {
                    res.push(hex as char);
                    iter.nth(1);
                }
            },
            _ => res.push(ch),
        }
    }
    res
}


/// (De)serializing bytestrings for JSON
pub fn ser_hex<S: Serializer>(v: &Vec<u8>, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&hex::encode(v))
}

/// (De)serializing bytestrings for JSON
pub fn de_hex<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<u8>, D::Error> {
    let s = <String as Deserialize>::deserialize(d)?;
    hex::decode(&s).map_err(|_| D::Error::custom("invalid hex string"))
}


/// Retrieve MAC address of a system interface.
pub fn retrieve_mac() -> Option<String> {
    for entry in fs::read_dir("/sys/class/net").ok()? {
        let path = entry.ok()?.path();
        // addr_assign_type 0 means that it is an actual permanent address.
        if let Ok("0\n") = fs::read_to_string(path.join("addr_assign_type")).as_deref() {
            if let Ok(addr) = fs::read_to_string(path.join("address")) {
                if !addr.ends_with(":00:00\n") {
                    return Some(addr.trim().into());
                }
            }
        }
    }
    None
}

/// Generate a display ID.  Tries /etc/machine-id, the DMI board id, the MAC or the hostname.
pub fn get_display_id() -> String {
    if let Ok(id) = fs::read_to_string("/etc/machine-id") {
        return id.trim().into();
    }
    // Try the DMI board id, the MAC address and the hostname.
    // Process all info into a big string and hash it.
    let mut buffer = [0u8; 64];
    let idstring = format!(
        "{:?}{:?}{:?}{:?}",
        fs::read_to_string("/sys/devices/virtual/dmi/id/board_name"),
        fs::read_to_string("/sys/devices/virtual/dmi/id/board_version"),
        retrieve_mac(),
        gethostname(&mut buffer).ok().and_then(|s| s.to_str().ok())
    );
    hex::encode(&Md5::digest(idstring.as_bytes()))
}


const SS_SVC: &str   = "org.freedesktop.ScreenSaver";
const SS_PATH: &str  = "/ScreenSaver";
const SS_IFACE: &str = "org.freedesktop.ScreenSaver";
const SS_METH: &str  = "Inhibit";

/// Inhibit the screensaver.
pub fn inhibit_screensaver() -> Result<u32> {
    let conn = Connection::new_session().context("connecting to dbus")?;
    let proxy = conn.with_proxy(SS_SVC, SS_PATH, Duration::from_millis(500));
    let res: (u32,) = proxy.method_call(SS_IFACE, SS_METH, ("Arexibo", "Showing signage"))?;
    Ok(res.0)
}
