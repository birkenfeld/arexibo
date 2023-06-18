// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Schedule parsing and scheduling.

use std::{cmp::Ordering, sync::Arc};
use anyhow::{Context, Result};
use time::OffsetDateTime;
use elementtree::Element;
use crate::resource::{Cache, LayoutInfo};
use crate::util::{TIME_FMT, ElementExt};

type Dt = OffsetDateTime;
type LayoutId = i64;

#[derive(Debug, Default)]
pub struct Schedule {
    default: Option<LayoutId>,
    schedules: Vec<(Dt, Dt, LayoutId, i32)>,
}

impl Schedule {
    pub fn parse(tree: Element) -> Result<Self> {
        let mut schedules = Vec::new();
        for layout in tree.find_all("layout") {
            let id = layout.parse_attr("file")?;
            let prio = layout.parse_attr("priority")?;
            let from = layout.get_attr("fromdt").context("missing fromdt")?;
            let to = layout.get_attr("todt").context("missing todt")?;
            let from = Dt::parse(from, &TIME_FMT).context("invalid fromdt")?;
            let to = Dt::parse(to, &TIME_FMT).context("invalid todt")?;
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

    pub fn layouts_now(&self, cache: &Cache) -> Vec<Arc<LayoutInfo>> {
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
                if let Some(info) = cache.get_layout(lid) {
                    layouts.push(info);
                }
            }
        }
        if layouts.is_empty() {
            if let Some(def) = self.default {
                if let Some(info) = cache.get_layout(def) {
                    layouts.push(info);
                }
            }
        }
        layouts
    }
}
