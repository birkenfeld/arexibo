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
div.region { position: absolute; }
.media { display: none; }
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
        writeln!(out, "<div class='region' id='r{}' style='\
                       width: {}px; height: {}px; left: {}px; top: {}px'>",
                 rid,
                 region.def_attr("width", "0"),
                 region.def_attr("height", "0"),
                 region.def_attr("left", "0"),
                 region.def_attr("top", "0"),
        )?;

        let mut sequence = Vec::new();
        for media in region.find_all("media") {
            let id = media.def_attr("id", "");
            let len = media.def_attr("duration", "").parse::<i32>().unwrap_or(10);
            let w = region.def_attr("width", "0");
            let h = region.def_attr("height", "0");
            let mut start = "".into();
            let mut trans = None;
            match (media.get_attr("render"), media.get_attr("type")) {
                (Some("html"), _) |
                (_, Some("text")) |
                (_, Some("ticker")) => {
                    // TODO: override duration (from HTML)
                    writeln!(out, "<iframe class='media' id='m{}' src='{}.html' width='{}' height='{}'></iframe>",
                             id, id, w, h)?;
                }
                (_, Some("webpage")) => {
                    writeln!(out, "<iframe class='media' id='m{}' src='{}' width='{}' height='{}'></iframe>",
                             id, percent_decode(media.find("options").unwrap().find("uri").unwrap().text()),
                             w, h)?;
                }
                (_, Some("image")) => {
                    // TODO: center image
                    writeln!(out, "<img class='media' id='m{}' src='{}' style='width: {}px; height: {}px; object-fit: contain;'>",
                             id, media.find("options").unwrap().find("uri").unwrap().text(), w, h)?;
                }
                (_, Some("video")) => {
                    let filename = media.find("options").unwrap().find("uri").unwrap().text();
                    writeln!(out, "<video class='media' id='m{}' src='{}' muted width='{}'></video>",
                             id, filename, w)?;
                    start = format!("$('#m{}')[0].play();", id);
                    trans = Some(format!("$('#m{}')[0].onended = () => {{ ### }};", id));
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

                writeln!(out, "$('#r{}').children().hide(); $('#m{}').show(); {}",
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
            writeln!(out, "$('#m{}').show(); {}", id, start)?;
        }
        writeln!(out, "</script>")?;

        // TODO: <options>

        writeln!(out, "</div>")?;
    }

    writeln!(out, "<script>$(document).ready(function() {{")?;
    for rid in animate_regions {
        writeln!(out, "r{}_s0();", rid)?;
    }
    writeln!(out, "}});</script>")?;

    writeln!(out, "</body></html>")?;
    Ok(())
}
