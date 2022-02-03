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

const SCRIPT: &str = r#"
"#;


pub fn translate(xlf: &Path, html: &Path) -> Result<()> {
    let file = fs::File::open(xlf)?;
    let tree = Element::from_reader(file).context("parsing XLF")?;

    let out = fs::File::create(html)?;
    let mut out = BufWriter::new(out);

    writeln!(out, "<!doctype html><html><head>")?;
    writeln!(out, "<meta charset='utf-8'>")?;
    writeln!(out, "<script src='jquery.min.js'></script>")?;
    writeln!(out, "<style type='text/css'>\n{}", LAYOUT_CSS)?;

    if let Some(file) = tree.get_attr("background") {
        writeln!(out, "body {{ background-image: url('{}'); }}", file)?;
    }
    if let Some(color) = tree.get_attr("bgcolor") {
        writeln!(out, "body {{ background-color: {}; }}", color)?;
    }

    writeln!(out, "</style>\n<script>\n{}</script>", SCRIPT)?;
    writeln!(out, "</head><body>")?;

    let mut animate_regions = Vec::new();
    for region in tree.find_all("region") {
        let rid = region.def_attr("id", "");
        let x = region.def_attr("left", "0");
        let y = region.def_attr("top", "0");
        let w = region.def_attr("width", "0");
        let h = region.def_attr("height", "0");

        let mut sequence = Vec::new();
        for media in region.find_all("media") {
            let id = media.def_attr("id", "");
            let len = media.def_attr("duration", "").parse::<i32>().unwrap_or(10);
            let opts = media.find("options").unwrap();
            let mut start = "".into();
            let mut trans = None;
            match (media.get_attr("render"), media.get_attr("type")) {
                (Some("html"), _) |
                (_, Some("text")) |
                (_, Some("ticker")) => {
                    // TODO: override duration (comes from media properties)
                    writeln!(out, "<iframe class='media r{}' id='m{}' src='{}.html' \
                                   style='left: {}px; top: {}px;' width='{}' height='{}'></iframe>",
                             rid, id, id, x, y, w, h)?;
                }
                (_, Some("webpage")) => {
                    let url = percent_decode(media.find("options").unwrap().find("uri").unwrap().text());
                    writeln!(out, "<iframe class='media r{}' id='m{}' src='{}' \
                                   style='left: {}px; top: {}px;' width='{}' height='{}'></iframe>",
                             rid, id, url, x, y, w, h)?;
                }
                (_, Some("image")) => {
                    // TODO: handle alignment within region
                    let filename = media.find("options").unwrap().find("uri").unwrap().text();
                    writeln!(out, "<img class='media r{}' id='m{}' src='{}' \
                                   style='left: {}px; top: {}px; width: {}px; height: {}px; {}{}'>",
                             rid, id, filename, x, y, w, h, object_fit(opts), object_pos(opts))?;
                }
                (_, Some("video")) => {
                    let filename = media.find("options").unwrap().find("uri").unwrap().text();
                    writeln!(out, "<video class='media r{}' id='m{}' src='{}' muted \
                                   style='left: {}px; top: {}px; {}{}' width='{}' height='{}'></video>",
                             rid, id, filename, x, y, object_fit(opts), object_pos(opts), w, h)?;
                    start = format!("$('#m{}')[0].play();", id);
                    trans = Some(format!("$('#m{}')[0].onended = (e) => {{ e.target.fastSeek(0); ### }};", id));
                }
                _ => continue,
            }
            sequence.push((id, len, start, trans));

            // TODO: <options>
        }

        writeln!(out, "<script>")?;
        if sequence.len() > 1 {

            for (i, (id, dur, start, trans)) in sequence.iter().enumerate() {
                writeln!(out, "function r{}_s{}() {{", rid, i)?;

                writeln!(out, "$('.r{}').css('visibility', 'hidden'); $('#m{}').css('visibility', 'visible'); {}",
                         rid, id, start)?;

                let next_fn = format!("r{}_s{}();", rid, if i == sequence.len() - 1 { 0 } else { i+1 });
                if *dur != 0 {
                    writeln!(out, "window.setTimeout(() => {{ {} }}, {});",
                             next_fn, dur*1000)?;
                }
                if let Some(t) = trans {
                    writeln!(out, "{}", t.replace("###", &next_fn))?;
                }
                writeln!(out, "}}")?;
            }
            animate_regions.push(rid);

        } else if let Some((id, _, start, _)) = sequence.pop() {
            writeln!(out, "$('#m{}').css('visibility', 'visible'); {}", id, start)?;
        }
        writeln!(out, "</script>")?;

        // TODO: <options>
    }

    writeln!(out, "<script>$(document).ready(function() {{")?;
    for rid in animate_regions {
        writeln!(out, "r{}_s0();", rid)?;
    }
    writeln!(out, "}});</script>")?;

    writeln!(out, "</body></html>")?;
    Ok(())
}

fn object_fit(el: &Element) -> &'static str {
    match el.def_attr("scaleType", "center") {
        "stretch" => "object-fit: fill; ",
        _ => "object-fit: contain; ",
    }
}

fn object_pos(el: &Element) -> &'static str {
    match (el.def_attr("align", "center"), el.def_attr("halign", "center")) {
        ("left", "top") => "object-position: left top; ",
        ("left", "bottom") => "object-position: left bottom; ",
        ("left", _) => "object-position: left; ",
        ("right", "top") => "object-position: right top; ",
        ("right", "bottom") => "object-position: right bottom; ",
        ("right", _) => "object-position: right; ",
        (_, "top") => "object-position: top; ",
        (_, "bottom") => "object-position: bottom; ",
        _ => "",
    }
}
