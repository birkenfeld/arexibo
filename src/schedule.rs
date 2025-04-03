// Xibo player Rust implementation, (c) 2022-2024 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Schedule parsing and scheduling.

use std::{cmp::Ordering, fs::File, path::Path};
use anyhow::{Context, Result};
use time::{OffsetDateTime, PrimitiveDateTime};
use elementtree::Element;
use serde::{Serialize, Deserialize};
use crate::util::{TIME_FMT, ElementExt};

type LayoutId = i64;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Schedule {
    default: Option<LayoutId>,
    schedules: Vec<(OffsetDateTime, OffsetDateTime, LayoutId, i32)>,
}

impl Schedule {
    pub fn parse(tree: Element) -> Result<Self> {
        let tz_offset = OffsetDateTime::now_local().unwrap().offset();
        let mut schedules = Vec::new();
        for layout in tree.find_all("layout") {
            let id = layout.parse_attr("file")?;
            let prio = layout.parse_attr("priority")?;
            let from = layout.get_attr("fromdt").context("missing fromdt")?;
            let to = layout.get_attr("todt").context("missing todt")?;
            let from = PrimitiveDateTime::parse(from, &TIME_FMT)
                .context("invalid fromdt")?
                .assume_offset(tz_offset);
            let to = PrimitiveDateTime::parse(to, &TIME_FMT)
                .context("invalid todt")?
                .assume_offset(tz_offset);
            schedules.push((from, to, id, prio));
        }
        let mut default = None;
        if let Some(def) = tree.find("default") {
            default = Some(def.parse_attr("file")?);
        }

        Ok(Self {
            default,
            schedules
        })
    }

    pub fn layouts_now(&self) -> Vec<i64> {
        let now = OffsetDateTime::now_local().unwrap();
        let mut cur_prio = 0;
        let mut layouts = Vec::new();
        for &(from, to, lid, prio) in &self.schedules {
            if from <= now && now <= to {
                match prio.cmp(&cur_prio) {
                    Ordering::Less => continue,
                    Ordering::Greater => {
                        cur_prio = prio;
                        layouts.clear();
                    }
                    _ => ()
                }
                layouts.push(lid);
            }
        }
        if layouts.is_empty() {
            if let Some(def) = self.default {
                layouts.push(def);
            }
        }
        layouts
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        serde_json::from_reader(File::open(path.as_ref())?)
            .context("deserializing schedule")
    }

    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        serde_json::to_writer_pretty(File::create(path.as_ref())?, self)
            .context("serializing schedule")
    }
}
