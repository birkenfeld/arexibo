// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! XLF layout parser and translator.

use std::{fs, io::{Write, BufWriter}};
use std::path::Path;
use anyhow::{Context, Result};
use elementtree::Element;
use crate::util::{ElementExt, percent_decode};

const LAYOUT_CSS: &str = r#"
body { margin: 0; background-repeat: no-repeat; overflow: hidden; }
iframe { border: 0 }
.media { position: absolute; visibility: hidden; }
p { margin-top: 0; }
"#;


pub struct Translator {
    tree: Option<Element>,
    out: BufWriter<fs::File>,
    animate_regions: Vec<i32>,
}

impl Translator {
    pub fn new(xlf: &Path, html: &Path) -> Result<Self> {
        let file = fs::File::open(xlf)?;
        let tree = Some(Element::from_reader(file).context("parsing XLF")?);

        let out = fs::File::create(html)?;
        let out = BufWriter::new(out);

        Ok(Self { tree, out, animate_regions: Vec::new() })
    }

    pub fn translate(mut self) -> Result<()> {
        let tree = self.tree.take().unwrap();
        self.write_header(&tree)?;
        for region in tree.find_all("region") {
            if let Err(e) = self.write_region(region) {
                log::error!("layout: could not translate region: {:#}", e);
            }
        }
        self.write_footer()?;
        Ok(())
    }

    fn write_header(&mut self, el: &Element) -> Result<()> {
        writeln!(self.out, "<!doctype html><html><head>")?;
        writeln!(self.out, "<meta charset='utf-8'>")?;
        writeln!(self.out, "<script src='jquery.min.js'></script>")?;
        writeln!(self.out, "<style type='text/css'>\n{}", LAYOUT_CSS)?;

        if let Some(file) = el.get_attr("background") {
            writeln!(self.out, "body {{ background-image: url('{}'); }}", file)?;
        }
        if let Some(color) = el.get_attr("bgcolor") {
            writeln!(self.out, "body {{ background-color: {}; }}", color)?;
        }

        writeln!(self.out, "</style>")?;
        writeln!(self.out, "</head><body>")?;
        Ok(())
    }

    fn write_footer(&mut self) -> Result<()> {
        // start all regions' first item
        writeln!(self.out, "<script>$(document).ready(function() {{")?;
        for rid in &self.animate_regions {
            writeln!(self.out, "r{}_s0();", rid)?;
        }
        writeln!(self.out, "}});</script>")?;
        // TODO: end of layout

        writeln!(self.out, "</body></html>")?;
        Ok(())
    }

    fn write_region(&mut self, region: &Element) -> Result<()> {
        let rid = region.parse_attr("id")?;
        let x = region.parse_attr("left")?;
        let y = region.parse_attr("top")?;
        let w = region.parse_attr("width")?;
        let h = region.parse_attr("height")?;
        let geom = [x, y, w, h];

        let mut sequence = Vec::new();
        for media in region.find_all("media") {
            // TODO: transitions
            match self.write_media(rid, geom, media) {
                Err(e) => log::error!("layout: could not translate media: {:#}", e),
                Ok(None) => continue,
                Ok(Some(res)) => sequence.push(res),
            }
        }

        writeln!(self.out, "<script>")?;
        if sequence.len() > 1 {
            for (i, (mid, duration, custom_start, custom_transition)) in sequence.iter().enumerate() {
                writeln!(self.out, "function r{}_s{}() {{", rid, i)?;
                writeln!(self.out, "$('.r{}').css('visibility', 'hidden'); \
                                    $('#m{}').css('visibility', 'visible'); \
                                    {}",
                         rid, mid, custom_start)?;

                let next_i = if i == sequence.len() - 1 { 0 } else { i+1 };
                let next_fn = format!("r{}_s{}();", rid, next_i);
                if let Some(tmpl) = custom_transition {
                    writeln!(self.out, "{}", tmpl.replace("###", &next_fn))?;
                }
                if *duration != 0 {
                    writeln!(self.out, "window.setTimeout(() => {{ {} }}, {});",
                             next_fn, 1000 * duration)?;
                }
                writeln!(self.out, "}}")?;
            }
            self.animate_regions.push(rid);

        } else if let Some((mid, _, custom_start, _)) = sequence.pop() {
            writeln!(self.out, "$('#m{}').css('visibility', 'visible'); {}",
                     mid, custom_start)?;
        }
        writeln!(self.out, "</script>")?;

        // TODO: options (loop, transition)
        Ok(())
    }

    fn write_media(&mut self, rid: i32, [x, y, w, h]: [i32; 4],
                   media: &Element) -> Result<Option<(i32, i32, String, Option<String>)>> {
        let mid = media.parse_attr("id")?;
        let opts = media.find("options").context("no options")?;
        let len = media.def_attr("duration", "").parse::<i32>().unwrap_or(10);
        let mut custom_start = "".into();
        let mut custom_transition = None;
        match (media.get_attr("render"), media.get_attr("type")) {
            (Some("html"), _) |
            (_, Some("text")) |
            (_, Some("ticker")) => {
                // TODO: override duration (comes from media properties)
                writeln!(self.out, "<iframe class='media r{}' id='m{}' src='{}.html' \
                                    style='left: {}px; top: {}px; width: {}px; \
                                    height: {}px;'></iframe>",
                         rid, mid, mid, x, y, w, h)?;
            }
            (_, Some("webpage")) => {
                let url = percent_decode(opts.find("uri").context("no web uri")?.text());
                writeln!(self.out, "<iframe class='media r{}' id='m{}' src='{}' \
                                    style='left: {}px; top: {}px; width: {}px; \
                                    height: {}px;'></iframe>",
                         rid, mid, url, x, y, w, h)?;
            }
            (_, Some("image")) => {
                let filename = opts.find("uri").context("no image uri")?.text();
                writeln!(self.out, "<img class='media r{}' id='m{}' src='{}' \
                                    style='left: {}px; top: {}px; width: {}px; \
                                    height: {}px;{}{}'>",
                         rid, mid, filename, x, y, w, h, object_fit(opts), object_pos(opts))?;
            }
            (_, Some("video")) => {
                let filename = opts.find("uri").context("no video uri")?.text();
                writeln!(self.out, "<video class='media r{}' id='m{}' src='{}' muted \
                                    style='left: {}px; top: {}px; width: {}px; \
                                    height: {}px;{}{}'></video>",
                         rid, mid, filename, x, y, w, h, object_fit(opts), object_pos(opts))?;
                custom_start = format!("$('#m{}')[0].play();", mid);
                custom_transition = Some(format!("$('#m{}')[0].onended = (e) => {{ \
                                                  e.target.fastSeek(0); ### }};", mid));
            }
            _ => {
                log::warn!("unsupported media type: {:?}", media.get_attr("type"));
                return Ok(None);
            }
        }
        Ok(Some((mid, len, custom_start, custom_transition)))
    }
}

fn object_fit(el: &Element) -> &'static str {
    match el.def_attr("scaleType", "center") {
        "stretch" => " object-fit: fill;",
        _ => " object-fit: contain;",
    }
}

fn object_pos(el: &Element) -> &'static str {
    match (el.def_attr("align", "center"), el.def_attr("halign", "center")) {
        ("left", "top") => " object-position: left top;",
        ("left", "bottom") => " object-position: left bottom;",
        ("left", _) => " object-position: left;",
        ("right", "top") => " object-position: right top;",
        ("right", "bottom") => " object-position: right bottom;",
        ("right", _) => " object-position: right;",
        (_, "top") => " object-position: top;",
        (_, "bottom") => " object-position: bottom;",
        _ => "",
    }
}
