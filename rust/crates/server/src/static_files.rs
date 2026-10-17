use axum::{
    body::Body,
    http::{HeaderMap, Response, StatusCode, Uri, header},
};
use flate2::{Compression, write::GzEncoder};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::UNIX_EPOCH,
};

#[derive(Clone)]
pub(crate) struct StaticFiles {
    root: PathBuf,
    zipped: Arc<Mutex<BTreeMap<PathBuf, Packed>>>,
}

struct Packed {
    tag: String,
    body: Vec<u8>,
}

impl StaticFiles {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            zipped: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub(crate) async fn serve(&self, uri: Uri, headers: HeaderMap) -> Response<Body> {
        let files = self.clone();
        match tokio::task::spawn_blocking(move || files.response(&uri, &headers)).await {
            Ok(Some(response)) => response,
            _ => not_found(),
        }
    }

    fn response(&self, uri: &Uri, headers: &HeaderMap) -> Option<Response<Body>> {
        let path = decode(uri.path(), false)?;
        let edit = uri.query().is_some_and(|query| {
            query.split('&').any(|part| {
                decode(part.split('=').next().unwrap_or(""), true).as_deref() == Some("edit")
            })
        });
        let (base, relative) = match path.as_str() {
            "/" if edit => ("client", "index.html"),
            "/play" => ("client", "index.html"),
            "/" => ("client", "public-lobby.html"),
            _ => [
                ("/client/", "client"),
                ("/shared/", "shared"),
                ("/vendor/", "node_modules/three/build"),
                ("/vendor-jsm/", "node_modules/three/examples/jsm"),
            ]
            .into_iter()
            .find_map(|(prefix, base)| path.strip_prefix(prefix).map(|rest| (base, rest)))?,
        };
        let relative = Path::new(relative);
        if !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            return None;
        }
        let root = self.root.canonicalize().ok()?;
        let base = root.join(base);
        if base.canonicalize().ok()? != base {
            return None;
        }
        let file = base.join(relative).canonicalize().ok()?;
        if !file.starts_with(&base) {
            return None;
        }
        let mut handle = File::open(&file).ok()?;
        let info = handle.metadata().ok()?;
        if !info.is_file() {
            return None;
        }
        let modified = info
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis();
        let tag = format!(
            "\"{}-{}\"",
            base36(u128::from(info.len())),
            base36(modified)
        );
        let mime = match relative
            .extension()
            .and_then(|extension| extension.to_str())
        {
            Some("html") => "text/html",
            Some("js") => "text/javascript",
            Some("css") => "text/css",
            Some("json") => "application/json",
            Some("png") => "image/png",
            Some("jpg") => "image/jpeg",
            Some("webp") => "image/webp",
            Some("svg") => "image/svg+xml",
            Some("mp3") => "audio/mpeg",
            _ => "application/octet-stream",
        };
        let text = mime.starts_with("text/") || mime.contains("json") || mime.contains("svg");
        let mut response = Response::builder()
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::ETAG, &tag);
        if text {
            response = response.header(header::VARY, "accept-encoding");
        }
        if headers
            .get(header::IF_NONE_MATCH)
            .is_some_and(|value| value.as_bytes() == tag.as_bytes())
        {
            return response
                .status(StatusCode::NOT_MODIFIED)
                .body(Body::empty())
                .ok();
        }
        let gzip = text
            && headers
                .get(header::ACCEPT_ENCODING)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| {
                    value
                        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                        .any(|word| word == "gzip")
                });
        let body = if gzip {
            let cached = self
                .zipped
                .lock()
                .ok()?
                .get(&file)
                .filter(|packed| packed.tag == tag)
                .map(|packed| packed.body.clone());
            if let Some(body) = cached {
                body
            } else {
                let mut bytes = Vec::new();
                handle.read_to_end(&mut bytes).ok()?;
                let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(&bytes).ok()?;
                let body = encoder.finish().ok()?;
                let mut cache = self.zipped.lock().ok()?;
                if cache.len() >= 256 && !cache.contains_key(&file) {
                    cache.pop_first();
                }
                cache.insert(
                    file,
                    Packed {
                        tag,
                        body: body.clone(),
                    },
                );
                body
            }
        } else {
            let mut bytes = Vec::new();
            handle.read_to_end(&mut bytes).ok()?;
            bytes
        };
        if gzip {
            response = response.header(header::CONTENT_ENCODING, "gzip");
        }
        response.status(StatusCode::OK).body(Body::from(body)).ok()
    }
}

pub(crate) fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("not found"))
        .unwrap()
}

fn base36(mut number: u128) -> String {
    let mut digits = Vec::new();
    loop {
        digits.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(number % 36) as usize]);
        number /= 36;
        if number == 0 {
            break;
        }
    }
    digits.reverse();
    String::from_utf8(digits).unwrap()
}

fn decode(text: &str, plus: bool) -> Option<String> {
    let bytes = text.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let pair = text.get(index + 1..index + 3)?;
            result.push(u8::from_str_radix(pair, 16).ok()?);
            index += 3;
        } else {
            result.push(if plus && bytes[index] == b'+' {
                b' '
            } else {
                bytes[index]
            });
            index += 1;
        }
    }
    String::from_utf8(result).ok()
}

#[cfg(test)]
#[path = "static_http_tests.rs"]
mod tests;
