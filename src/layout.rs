// Xibo player Rust implementation, (c) 2022-2024 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! XLF layout parser and translator.

use std::{fs, io::{Write, BufWriter}, collections::HashMap};
use std::path::Path;
use anyhow::{Context, Result};
use elementtree::Element;
use crate::resource::LayoutId;
use crate::util::{ElementExt, percent_decode};

// TODO:
// - transitions
// - reloading resources in iframes
// - overriding duration from resources
// - fromDt/toDt

pub const TRANSLATOR_VERSION: u32 = 9;

const LAYOUT_CSS: &str = r#"
body { margin: 0; background-repeat: no-repeat; overflow: hidden; }
iframe { border: 0 }
.media { position: absolute; visibility: hidden; }
p { margin-top: 0; }
"#;

const SCRIPT: &str = r#"
new QWebChannel(qt.webChannelTransport, function(channel) {
  window.arexiboGui = channel.objects.arexibo;
  window.arexiboGui.jsLayoutInit(window.arexibo.id,
                                 window.arexibo.width, window.arexibo.height);
});

window.arexibo = {
  id: 0,
  width: 0,
  height: 0,
  done: false,
  regions_total: 0,
  triggers: {},
  regions: {},

  region_switch: function(rid, next, first) {
    let {cur, total, timeoutid, media} = this.regions[rid];
    // stop a timeout, if it still exists
    window.clearTimeout(timeoutid);

    // determine next media
    if (next == -1)
      next = (cur + 1) % total;
    else if (next == -2)
      next = (cur + total - 1) % total;

    // stop showing the current media
    if (cur !== null)
      media[cur][1]();

    this.regions[rid].cur = next;
    // when the first media is called for the second time, the region is "done"
    if (next == 0 && !first) {
      this.region_done(rid);
    }

    // start showing the next media
    media[next][0]();

    // set timeout to switch to the next media
    let duration = media[next][2]() || 1;
    this.regions[rid].timeoutid = window.setTimeout(() => {
      this.region_switch(rid, -1, false);
    }, duration * 1000);
  },

  region_done: function(rid) {
    if (this.done) return;

    this.regions[rid].done = true;
    // check if all regions are done
    for (let region of Object.values(this.regions)) {
      if (!region.done) return;
    }
    window.arexiboGui.jsLayoutDone(window.arexibo.id);
    this.done = true;
  },

  trigger: function(code) {
    if (this.triggers[code] !== undefined) {
      let {action, target, targetid, layoutid} = this.triggers[code];
      if (action == 'navLayout') {
        window.arexiboGui.jsLayoutJump(window.arexibo.id, layoutid);
      } else if (action == 'previous' || action == 'next') {
        if (target == 'layout') {
          if (action == 'next')
            window.arexiboGui.jsLayoutDone(window.arexibo.id);
          else
            window.arexiboGui.jsLayoutPrev(window.arexibo.id);
        } else {
          if (action == 'next')
            this.region_switch(targetid, -1);
          else
            this.region_switch(targetid, -2);
        }
      }
    }
  },
};
"#;


type MediaInfo = (i32, String, String, String);

pub struct Translator<'a> {
    id: LayoutId,
    tree: Option<Element>,
    out: BufWriter<fs::File>,
    regions: Vec<i32>,
    size: (i32, i32),
    code_map: &'a HashMap<String, LayoutId>,
}

impl<'a> Translator<'a> {
    pub fn new(id: LayoutId, xlf: &Path, html: &Path,
               code_map: &'a HashMap<String, LayoutId>) -> Result<Self> {
        let file = fs::File::open(xlf)?;
        let tree = Some(Element::from_reader(file).context("parsing XLF")?);

        let out = fs::File::create(html)?;
        let out = BufWriter::new(out);

        Ok(Self { id, tree, out, regions: Vec::new(), size: (0, 0), code_map })
    }

    pub fn translate(mut self) -> Result<(i32, i32)> {
        let tree = self.tree.take().unwrap();
        self.write_header(&tree)?;
        for region in tree.find_all("region") {
            if let Err(e) = self.write_region(region) {
                log::error!("layout: could not translate region: {:#}", e);
            }
        }
        writeln!(self.out, "<script type='text/javascript'>")?;
        for action in tree.find_all("action") {
            if let Err(e) = self.write_action(action) {
                log::error!("layout: could not translate action: {:#}", e);
            }
        }
        writeln!(self.out, "</script>")?;
        self.write_footer()?;
        Ok(self.size)
    }

    fn write_action(&mut self, el: &Element) -> Result<()> {
        let typ = el.req_attr("triggerType")?;
        let action = el.req_attr("actionType")?;
        let target = el.req_attr("target")?;
        let targetid = el.parse_attr::<i64>("targetId")?;
        let code = el.def_attr("triggerCode", "<not set>");
        let layoutcode = el.def_attr("layoutCode", "<not set>");
        let mut layoutid = 0;
        if action == "navLayout" {
            layoutid = self.code_map.get(layoutcode).cloned().context("unknown layout code")?;
        }
        if typ == "webhook" {
            writeln!(self.out, "window.arexibo.triggers[{code:?}] = {{")?;
            writeln!(self.out, "  action: {action:?},")?;
            writeln!(self.out, "  target: {target:?},")?;
            writeln!(self.out, "  targetid: {targetid},")?;
            writeln!(self.out, "  layoutid: {layoutid}")?;
            writeln!(self.out, "}};")?;
        } else if typ == "touch" {
            // TODO
            log::warn!("touch actions not yet supported");
        } else {
            log::warn!("unsupported action type: {typ}");
        }
        Ok(())
    }

    fn write_header(&mut self, el: &Element) -> Result<()> {
        self.size = (el.parse_attr("width")?, el.parse_attr("height")?);

        writeln!(self.out, "<!DOCTYPE html>\n<!-- VERSION={} -->", TRANSLATOR_VERSION)?;
        writeln!(self.out, "<html><head>")?;
        writeln!(self.out, "<meta charset='utf-8'>")?;
        writeln!(self.out, "<script src='qrc:///qtwebchannel/qwebchannel.js'></script>")?;
        writeln!(self.out, "<script type='text/javascript'>{}\
                            window.arexibo.id = {};\n\
                            window.arexibo.width = {};\n\
                            window.arexibo.height = {};\n\
                            </script>", SCRIPT, self.id, self.size.0, self.size.1)?;
        writeln!(self.out, "<style type='text/css'>{}", LAYOUT_CSS)?;

        if let Some(file) = el.get_attr("background") {
            writeln!(self.out, "body {{ background-image: url('{file}'); \
                                background-size: 100vw 100vh; }}")?;
        }
        if let Some(color) = el.get_attr("bgcolor") {
            writeln!(self.out, "body {{ background-color: {color}; }}")?;
        }

        writeln!(self.out, "</style>")?;
        writeln!(self.out, "</head><body>")?;
        Ok(())
    }

    fn write_footer(&mut self) -> Result<()> {
        // start all regions' first item
        writeln!(self.out, "<script type='text/javascript'>\n\
                            window.addEventListener('load', function() {{")?;
        for rid in &self.regions {
            writeln!(self.out, "  window.arexibo.region_switch({rid}, 0, true);")?;
        }
        writeln!(self.out, "}});\n</script>")?;
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
        writeln!(self.out, "<!-- region {} -->", rid)?;

        if let Some(zindex) = region.get_attr("zindex") {
            writeln!(self.out, "<style type='text/css'> \
                                .r{rid} {{ z-index: {zindex}; }} \
                                </style>")?;
        }

        let mut sequence = Vec::new();
        for media in region.find_all("media") {
            match self.write_media(rid, geom, media) {
                Err(e) => log::error!("layout: could not translate media: {:#}", e),
                Ok(None) => continue,
                Ok(Some(res)) => sequence.push(res),
            }
        }
        let nitems = sequence.len();

        if nitems == 0 {
            return Ok(());
        }

        writeln!(self.out, "<script type='text/javascript'>")?;
        writeln!(self.out, "window.arexibo.regions[{rid}] = {{")?;
        writeln!(self.out, "  done: false,")?;
        writeln!(self.out, "  cur: null,")?;
        writeln!(self.out, "  timeoutid: null,")?;
        writeln!(self.out, "  total: {nitems},")?;
        writeln!(self.out, "  media: [")?;

        // for each media, write functions to start/stop displaying it
        for (mid, duration, add_start, add_stop) in sequence {
            writeln!(self.out, "    [function() {{")?;
            writeln!(self.out, "      document.getElementById('m{mid}').style.\
                                visibility = 'visible';")?;
            writeln!(self.out, "      {add_start}")?;
            writeln!(self.out, "    }}, function() {{")?;
            // if only one item is present, don't need to hide the others
            if nitems > 1 {
                writeln!(self.out, "      document.getElementById('m{mid}').style.\
                                    visibility = 'hidden'; ")?;
            }
            writeln!(self.out, "      {add_stop}")?;
            writeln!(self.out, "    }}, {duration}],")?;
        }
        writeln!(self.out, "  ],")?;
        writeln!(self.out, "}};\n</script>")?;
        self.regions.push(rid);
        Ok(())
    }

    fn write_media(&mut self, rid: i32, [x, y, w, h]: [i32; 4],
                   media: &Element) -> Result<Option<MediaInfo>> {
        let mid = media.parse_attr("id")?;
        let opts = media.find("options").context("no options")?;
        let mut duration = format!(
            "() => {}", media.def_attr("duration", "").parse::<i32>().unwrap_or(10));
        let mut add_start = "".into();
        let mut add_stop = "".into();
        writeln!(self.out, "  <!-- media {mid} -->")?;
        match (media.get_attr("render"), media.get_attr("type")) {
            (Some("html"), _) |
            (_, Some("text" | "ticker")) => {
                writeln!(self.out, "<iframe class='media r{rid}' id='m{mid}' \
                                    src='{mid}.html?w={w}&h={h}' \
                                    style='left: {x}px; top: {y}px; width: {w}px; \
                                    height: {h}px;'></iframe>")?;
            }
            (_, Some("webpage")) => {
                let url = percent_decode(opts.find("uri").context("no web uri")?.text());
                writeln!(self.out, "<iframe class='media r{rid}' id='m{mid}' src='{url}' \
                                    style='left: {x}px; top: {y}px; width: {w}px; \
                                    height: {h}px;'></iframe>")?;
            }
            (_, Some("pdf")) => {
                let filename = opts.find("uri").context("no pdf uri")?.text();
                writeln!(self.out, "<iframe class='media r{rid}' id='m{mid}' src='{filename}' \
                                    style='left: {x}px; top: {y}px; width: {w}px; \
                                    height: {h}px;'></iframe>")?;
            }
            (_, Some("image")) => {
                let filename = opts.find("uri").context("no image uri")?.text();
                writeln!(self.out, "<img class='media r{rid}' id='m{mid}' src='{filename}' \
                                    style='left: {x}px; top: {y}px; width: {w}px; \
                                    height: {h}px;{}{}'>",
                         object_fit(opts), object_pos(opts))?;
            }
            (_, Some("video")) | (_, Some("localvideo")) => {
                let url = percent_decode(opts.find("uri").context("no video uri")?.text());
                let mute = opts.find("mute").map_or(false, |el| el.text() == "1");
                writeln!(self.out, "<video class='media r{rid}' id='m{mid}' src='{url}' {} \
                                    style='left: {x}px; top: {y}px; width: {w}px; \
                                    height: {h}px;{}{}'></video>",
                         if mute { "muted" } else { "" },
                         object_fit(opts), object_pos(opts))?;
                add_start = format!("document.getElementById('m{}').play();", mid);
                duration = format!("() => document.getElementById('m{}').duration", mid);
            }
            (_, Some("shellcommand")) => {
                writeln!(self.out, "<div class='media r{rid}' id='m{mid}' \
                                    style='left: {x}px; top: {y}px; width: {w}px; \
                                    height: {h}px;'></div>")?;

                let is_cmd = opts.req_child("commandType")? == "storedCommand";
                if is_cmd {
                    let code = opts.req_child("commandCode")?;
                    add_start = format!("window.arexiboGui.jsCommand({code:?});");
                } else {
                    let with_shell = opts.req_child("launchThroughCmd")? == "1";
                    let cmd = if opts.req_child("useGlobalCommand")? == "1" {
                        opts.req_child("globalCommand")?
                    } else {
                        opts.req_child("linuxCommand")?
                    };
                    add_start = format!("window.arexiboGui.jsShell({cmd:?}, {with_shell});");

                    let kill = if opts.req_child("terminateCommand")? == "1" {
                        if opts.req_child("useTaskkill")? == "1" { 2 } else { 1 }
                    } else { 0 };
                    add_stop = format!("window.arexiboGui.jsStopShell({kill});");
                }
            }
            _ => {
                log::warn!("unsupported media type: {:?}", media.get_attr("type"));
                return Ok(None);
            }
        }
        Ok(Some((mid, duration, add_start, add_stop)))
    }
}

fn object_fit(el: &Element) -> &'static str {
    match el.find("scaleType") {
        Some(e) if e.text() == "stretch" => " object-fit: fill;",
        _ => " object-fit: contain;",
    }
}

fn object_pos(el: &Element) -> &'static str {
    match (el.def_attr("align", "center"), el.def_attr("halign", "middle")) {
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
