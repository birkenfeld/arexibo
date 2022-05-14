// Xibo player Rust implementation, (c) 2022 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Internal webserver to point the webview to.

use std::{sync::Arc, fs, io::Read, io::Seek, thread};
use std::path::{Path, PathBuf};
use anyhow::{anyhow, bail, ensure, Result};
use itertools::Itertools;
use tiny_http::{Request, Response, ResponseBox, Header, StatusCode};


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

    /// Serve a single HTTP request.
    fn serve(dir: &Path, req: &Request) -> Result<ResponseBox> {
        log::debug!("HTTP request: {}", req.url());
        Ok(match req.url() {
            // built-in files?
            "/splash.jpg" => Response::from_data(SPLASH_JPG).boxed(),
            "/0.xlf.html" => Response::from_data(SPLASH_HTML).boxed(),

            // any other static files
            path => {
                let path = dir.join(&path[1..]);
                if !path.is_file() {
                    log::warn!("processing HTTP req {}: 404 not found", req.url());
                    return Ok(Response::empty(404).boxed());
                }
                let mut fp = fs::File::open(&path)?;

                // implement HTTP Range query for gstreamer
                for h in req.headers() {
                    if h.field.equiv("Range") {
                        let total_size = fp.metadata()?.len();
                        let (from, to, size) = parse_range(total_size,
                                                           h.value.to_string())?;
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

                // guess the MIME type based on filename
                let ctype = match path.extension().and_then(|e| e.to_str()) {
                    Some("html") => "text/html",
                    Some("js") => "text/javascript",
                    Some("ttf" | "otf") => "application/font-sfnt",
                    Some("jpg" | "jpeg") => "image/jpeg",
                    Some("png") => "image/png",
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
<img style="display: block; width: 100%; height: 100%" src="splash.jpg">
</body>
</html>
"#;

const SPLASH_JPG: &[u8] = include_bytes!("../assets/splash.jpg");


/// Parse a HTTP Range header.
fn parse_range(total_size: u64, header: String) -> Result<(u64, u64, u64)> {
    let mut parts = header.split(&['=', '-'][..]);
    let (from, to) = match parts.next_tuple() {
        Some(("bytes", from, to)) => {
            (from.parse().unwrap_or(0), to.parse().unwrap_or(total_size - 1))
        }
        _ => bail!("invalid Range header")
    };
    ensure!(from <= to && to < total_size, "invalid Range from/to");
    let size = to - from + 1;
    Ok((from, to, size))
}
