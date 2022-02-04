// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Various utilities.

use std::{fmt, str::FromStr};
use anyhow::{Context, Result};


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
