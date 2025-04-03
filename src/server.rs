// Xibo player Rust implementation, (c) 2022-2024 Georg Brandl.
// Licensed under the GNU AGPL, version 3 or later.

//! Internal webserver to point the webview to.

use std::{sync::Arc, fs, io::Read, io::Seek, thread, collections::HashMap};
use std::path::{Path, PathBuf};
use anyhow::{anyhow, bail, ensure, Result};
use itertools::Itertools;
use tiny_http::{Request, Response, ResponseBox, Header, StatusCode};

use crate::util::percent_decode;


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

    pub fn port(&self) -> u16 {
        self.server.server_addr().to_ip().expect("IP address").port()
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
            "/favicon.ico" => Response::from_data(b"").boxed(),
            "/splash.jpg" => Response::from_data(SPLASH_JPG).boxed(),
            "/0.xlf.html" => Response::from_data(SPLASH_HTML).boxed(),

            // any other static files
            url => {
                let parts = url.split('?').collect_vec();
                let path = dir.join(&parts[0][1..]);
                let ext = path.extension().and_then(|e| e.to_str());

                let query_params = parts.get(1).map(|par| par.split('&').map(|p| {
                    let mut kv = p.split('=');
                    let k = percent_decode(kv.next().unwrap_or(""));
                    let v = percent_decode(kv.next().unwrap_or(""));
                    (k, v)
                }).collect::<HashMap<_, _>>()).unwrap_or_default();

                if !path.is_file() {
                    log::warn!("processing HTTP req {}: 404 not found", req.url());
                    return Ok(Response::empty(404).boxed());
                }
                let mut fp = fs::File::open(&path)?;

                // implement replacing [[ViewPortWidth]] by requested width
                if ext == Some("html") && query_params.contains_key("w") {
                    let mut data = Vec::new();
                    fp.read_to_end(&mut data)?;
                    if let Some(i) = (0..data.len())
                        .find(|&i| data[i..].starts_with(b"[[ViewPortWidth]]")) {
                        let mut new_data = data[..i].to_vec();
                        new_data.extend_from_slice(query_params["w"].as_bytes());
                        new_data.extend_from_slice(&data[i + 17..]);
                        data = new_data;
                    }

                    return Ok(Response::from_data(data)
                        .with_header(Header::from_bytes(b"Content-Type",
                                                        b"text/html").unwrap())
                        .boxed());
                }

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
                                Header::from_bytes(b"Content-Range", range).unwrap(),
                                Header::from_bytes(b"Content-Type", b"video/mp4").unwrap(),
                            ],
                            stream,
                            Some(size as usize),
                            None
                        ).with_chunked_threshold(usize::MAX).boxed());
                    }
                }

                // guess the MIME type based on filename
                let ctype = match ext {
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
                    .with_header(Header::from_bytes(b"Content-Type", ctype).unwrap())
                    .boxed()
            }
        })
    }
}

const SPLASH_HTML: &[u8] = br#"<!DOCTYPE html>
<html>
<head>
<script src="qrc:///qtwebchannel/qwebchannel.js"></script>
<script>
new QWebChannel(qt.webChannelTransport, function(channel) {
  window.arexiboGui = channel.objects.arexibo;
  window.arexiboGui.jsLayoutInit(0, 1920, 1080);
});
</script>
</head>
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
