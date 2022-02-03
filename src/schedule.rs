// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Schedule parsing and scheduling.

use anyhow::{Context, Result};
use chrono::{NaiveDateTime, Local};
use elementtree::Element;
use crate::util::ElementExt;

type Dt = NaiveDateTime;
type Layout = i64;

#[derive(Debug, Default)]
pub struct Schedule {
    default: Option<i64>,
    schedules: Vec<(Dt, Dt, Layout, i64)>,
}

const FMT: &str = "%Y-%m-%d %H:%M:%S";

impl Schedule {
    pub fn parse(tree: Element) -> Result<Self> {
        let mut schedules = Vec::new();
        for layout in tree.find_all("layout") {
            let id = layout.parse_attr("file")?;
            let prio = layout.parse_attr("priority")?;
            let from = layout.get_attr("fromdt").context("missing fromdt")?;
            let to = layout.get_attr("todt").context("missing todt")?;
            let from = NaiveDateTime::parse_from_str(from, FMT).context("invalid fromdt")?;
            let to = NaiveDateTime::parse_from_str(to, FMT).context("invalid todt")?;
            schedules.push((from, to, id, prio));
        }
        let mut default = None;
        for def in tree.find("default") {
            default = Some(def.parse_attr("file")?);
        }

        Ok(Self {
            default,
            schedules
        })
    }

    pub fn layouts_now(&self) -> Vec<Layout> {
        let now = Local::now().naive_local();
        let mut cur_prio = 0;
        let mut layouts = Vec::new();
        for &(from, to, layout, prio) in &self.schedules {
            if from >= now && now <= to {
                if prio < cur_prio {
                    continue;
                } else if prio > cur_prio {
                    cur_prio = prio;
                    layouts.clear();
                }
                layouts.push(layout);
            }
        }
        if layouts.is_empty() {
            if let Some(def) = self.default {
                layouts.push(def);
            }
        }
        layouts
    }
}
