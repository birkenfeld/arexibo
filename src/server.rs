// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Handling resources such as media and layout files.

use std::{sync::Arc, fs, io::Read, io::Seek, thread};
use std::path::PathBuf;
use anyhow::{anyhow, bail, Result};
use tiny_http::{Request, Response, ResponseBox, Header, StatusCode};

/// Internal webserver that is used to serve layouts and media to the webview.
pub struct Server {
    dir: PathBuf,
    server: tiny_http::Server,
}

impl Server {
    pub fn new(dir: PathBuf, port: u16) -> Result<Self> {
        let server = tiny_http::Server::http(("127.0.0.1", port))
            .map_err(|e| anyhow!(e))?;
        Ok(Self { dir, server })
    }

    pub fn start_pool(self) {
        let server = Arc::new(self.server);
        for _ in 0..4 {
            let server = server.clone();
            let dir = self.dir.clone();
            thread::spawn(move || {
                loop {
                    let req = server.recv().unwrap();
                    match Self::serve(&dir, &req) {
                        Ok(resp) => {  let _ = req.respond(resp); }
                        Err(e) => {
                            log::warn!("processing HTTP req {}: {:#}", req.url(), e);
                            let _ = req.respond(Response::empty(500));
                        }
                    }
                }
            });
        }
    }

    fn serve(dir: &PathBuf, req: &Request) -> Result<ResponseBox> {
        log::debug!("HTTP request: {}", req.url());
        Ok(match req.url() {
            "/splash.jpg" => Response::from_data(SPLASH_JPG).boxed(),
            "/splash.html" => Response::from_data(SPLASH_HTML).boxed(),
            path if path.starts_with("/layout?") => {
                let id: i64 = path[8..].parse()?;
                let htmlpath = dir.join(format!("{}.xlf.html", id));
                if !htmlpath.is_file() {
                    let xlfpath = dir.join(format!("{}.xlf", id));
                    if xlfpath.is_file() {
                        log::info!("requested layout {}, needs processing", id);
                        // layout::translate(&xlfpath, &htmlpath)?;
                        //     .context("translating layout")?;
                        Response::from_file(fs::File::open(htmlpath)?).boxed()
                    } else {
                        Response::empty(404).boxed()
                    }
                } else {
                    Response::from_file(fs::File::open(htmlpath)?).boxed()
                }
            },
            path => {
                let path = dir.join(&path[1..]);
                if !path.is_file() {
                    return Ok(Response::empty(404).boxed());
                }
                let mut fp = fs::File::open(&path)?;

                // implement HTTP Range query for gstreamer
                for h in req.headers() {
                    if h.field.equiv("Range") {
                        let total_size = fp.metadata()?.len();
                        let requested = h.value.to_string();
                        let mut parts = requested.split(&['=', '-'][..]);
                        let (from, to) = match (parts.next(), parts.next(), parts.next()) {
                            (Some("bytes"), Some(from), Some(to)) => {
                                (from.parse().unwrap_or(0), to.parse().unwrap_or(total_size - 1))
                            }
                            _ => bail!("invalid Range header")
                        };
                        if ! (from <= to && to < total_size) {
                            bail!("invalid Range from/to")
                        }
                        let size = to - from + 1;
                        fp.seek(std::io::SeekFrom::Start(from))?;
                        let stream = fp.take(size);

                        let range = format!("bytes {}-{}/{}", from, to, total_size);
                        return Ok(Response::new(
                            StatusCode(206),
                            vec![
                                Header::from_bytes(&b"Content-Range"[..],
                                                   range.as_bytes()).unwrap(),
                            ],
                            stream,
                            Some(size as usize),
                            None
                        ).boxed());
                    }
                }

                let ctype = match path.extension().and_then(|e| e.to_str()) {
                    Some("html") => "text/html",
                    Some("js") => "text/javascript",
                    Some("ttf") | Some("otf") => "application/font-sfnt",
                    Some("png") => "image/png",
                    Some("jpg") | Some("jpeg") => "image/jpeg",
                    Some("pdf") => "application/pdf",
                    Some("mp4") => "video/mp4",
                    Some("avi") => "video/avi",
                    Some("ogv") => "video/ogg",
                    Some("webm") => "video/webm",
                    _ => "",
                };

                Response::from_file(fp)
                    // for gstreamer, need a response with Content-Length => no chunked
                    .with_chunked_threshold(usize::MAX)
                    .with_header(Header::from_bytes(&b"Content-Type"[..], ctype.as_bytes()).unwrap())
                    .boxed()
            }
        })
    }
}

const SPLASH_HTML: &[u8] = br#"<!doctype html>
<html>
<body style="margin: 0">
<img style="width: 100%; height: 100%" src="splash.jpg">
</body>
</html>
"#;

const SPLASH_JPG: &[u8] = include_bytes!("../assets/splash.jpg");


// TODO:
// - central + persisted storage of media info, layout info
