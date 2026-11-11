//! What the server answers (spec R6.1): the web app's files, `/config.json`
//! from the settings, `/healthz`, and nothing else. No state, no cookies,
//! nothing read from a request but its method, its path and three headers.
use crate::settings::Settings;

/// One file of the web app, packed into the program by build.rs.
pub struct File {
    pub path: &'static str,
    pub raw: &'static [u8],
    pub br: Option<&'static [u8]>,
    pub gz: Option<&'static [u8]>,
    /// names the content, for the ETag
    pub tag: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/files.rs"));

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// True when this program holds the stand-in page, not a built Hubchat Web.
pub const STAND_IN: bool = cfg!(stand_in);

/// What the page may load and call: itself, and hubs over https (or on the
/// same machine). The same policy the development server sends
/// (tools/web-static.mjs; a test here keeps the two equal).
pub const CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; media-src blob:; connect-src https: http://localhost:* http://127.0.0.1:*; worker-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'";
/// The camera is the page's own (scanning a link's QR code); nothing else.
pub const PERMISSIONS: &str = "camera=(self), microphone=(), geolocation=()";

pub enum Body {
    Fixed(&'static [u8]),
    Made(Vec<u8>),
}

impl Body {
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        match self {
            Body::Fixed(b) => b.len(),
            Body::Made(b) => b.len(),
        }
    }
}

pub struct Reply {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    /// what a GET sends; a HEAD sends the same headers and no body
    pub body: Body,
}

pub struct Request<'a> {
    pub method: &'a str,
    /// the path alone, without the query
    pub path: &'a str,
    pub accept_encoding: Option<&'a str>,
    pub if_none_match: Option<&'a str>,
}

pub struct Site {
    files: &'static [File],
    config: Vec<u8>,
    health: Vec<u8>,
    https: bool,
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "wasm" => "application/wasm",
        "json" | "map" => "application/json",
        "webmanifest" => "application/manifest+json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Whether `Accept-Encoding` takes `coding` (a weight of 0 refuses it).
fn accepts(header: &str, coding: &str) -> bool {
    header.split(',').any(|part| {
        let mut it = part.split(';');
        let name = it.next().unwrap_or("").trim();
        let q = it.find_map(|p| p.trim().strip_prefix("q=").map(|q| q.trim().parse::<f32>().unwrap_or(0.0))).unwrap_or(1.0);
        name.eq_ignore_ascii_case(coding) && q > 0.0
    })
}

/// Whether `If-None-Match` names this content, in whichever compression it was kept.
fn same(header: &str, tag: &str) -> bool {
    header.split(',').any(|t| {
        let t = t.trim().trim_start_matches("W/").trim_matches('"');
        t == "*" || t.split('-').next() == Some(tag)
    })
}

impl Site {
    pub fn new(settings: &Settings) -> Site {
        Site::with_files(FILES, settings)
    }

    pub fn with_files(files: &'static [File], settings: &Settings) -> Site {
        let config = serde_json::json!({
            "name": settings.name,
            "suggested_hubs": settings.suggested_hubs,
            "hub_suggestions": settings.hub_suggestions,
            "version": VERSION,
        });
        let health = serde_json::json!({ "ok": true, "app": "hubchat-web", "version": VERSION });
        Site { files, config: config.to_string().into_bytes(), health: health.to_string().into_bytes(), https: settings.tls.is_some() }
    }

    fn reply(&self, status: u16, mut headers: Vec<(&'static str, String)>, body: Body) -> Reply {
        headers.push(("content-security-policy", CSP.into()));
        headers.push(("x-content-type-options", "nosniff".into()));
        headers.push(("referrer-policy", "no-referrer".into()));
        headers.push(("cross-origin-opener-policy", "same-origin".into()));
        headers.push(("permissions-policy", PERMISSIONS.into()));
        if self.https {
            headers.push(("strict-transport-security", "max-age=31536000".into()));
        }
        Reply { status, headers, body }
    }

    fn text(&self, status: u16, text: &'static str, mut headers: Vec<(&'static str, String)>) -> Reply {
        headers.push(("content-type", "text/plain; charset=utf-8".into()));
        headers.push(("cache-control", "no-store".into()));
        self.reply(status, headers, Body::Fixed(text.as_bytes()))
    }

    fn json(&self, body: &[u8], cache: &'static str) -> Reply {
        self.reply(200, vec![("content-type", "application/json".into()), ("cache-control", cache.into())], Body::Made(body.to_vec()))
    }

    pub fn answer(&self, req: &Request) -> Reply {
        if req.method != "GET" && req.method != "HEAD" {
            return self.text(405, "only GET and HEAD\n", vec![("allow", "GET, HEAD".into())]);
        }
        match req.path {
            "/healthz" => return self.json(&self.health, "no-store"),
            // from the settings, whatever the built page holds under that name
            "/config.json" => return self.json(&self.config, "no-cache"),
            _ => {}
        }
        let path = if req.path == "/" { "/index.html" } else { req.path };
        let Ok(at) = self.files.binary_search_by(|f| f.path.cmp(path)) else {
            return self.text(404, "not found\n", Vec::new());
        };
        let file = &self.files[at];
        // Vite puts a hash of the content in the name of everything under assets/
        let cache = if path.starts_with("/assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
        let enc = req.accept_encoding.unwrap_or("");
        let (body, coding) = match (file.br, file.gz) {
            (Some(br), _) if accepts(enc, "br") => (br, Some("br")),
            (_, Some(gz)) if accepts(enc, "gzip") => (gz, Some("gzip")),
            _ => (file.raw, None),
        };
        let mut headers = vec![("cache-control", cache.to_string())];
        headers.push(("etag", match coding {
            Some(c) => format!("\"{}-{c}\"", file.tag),
            None => format!("\"{}\"", file.tag),
        }));
        if file.br.is_some() || file.gz.is_some() {
            headers.push(("vary", "Accept-Encoding".into()));
        }
        if req.if_none_match.is_some_and(|h| same(h, file.tag)) {
            return self.reply(304, headers, Body::Fixed(b""));
        }
        headers.push(("content-type", content_type(path).into()));
        if let Some(c) = coding {
            headers.push(("content-encoding", c.into()));
        }
        self.reply(200, headers, Body::Fixed(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_FILES: &[File] = &[
        File { path: "/assets/app-1a2b3c.js", raw: b"console.log('a long enough script')", br: Some(b"BR-JS"), gz: Some(b"GZ-JS"), tag: "00000000000000aa" },
        File { path: "/assets/core-9f8e7d.wasm", raw: b"\0asm-raw", br: Some(b"BR-WASM"), gz: None, tag: "00000000000000bb" },
        File { path: "/icons/icon-192.png", raw: b"PNG", br: None, gz: None, tag: "00000000000000cc" },
        File { path: "/index.html", raw: b"<!doctype html><title>Hubchat</title>", br: Some(b"BR-HTML"), gz: Some(b"GZ-HTML"), tag: "00000000000000dd" },
        File { path: "/manifest.webmanifest", raw: b"{}", br: None, gz: None, tag: "00000000000000ee" },
        File { path: "/sw.js", raw: b"self.x=1", br: None, gz: Some(b"GZ-SW"), tag: "00000000000000ff" },
    ];

    fn settings(pairs: &[(&str, &str)]) -> Settings {
        let pairs: Vec<(String, String)> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Settings::from_env(move |name| pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())).unwrap()
    }
    fn site(pairs: &[(&str, &str)]) -> Site {
        Site::with_files(TEST_FILES, &settings(pairs))
    }
    fn get<'a>(path: &'a str) -> Request<'a> {
        Request { method: "GET", path, accept_encoding: None, if_none_match: None }
    }
    fn header<'a>(r: &'a Reply, name: &str) -> Option<&'a str> {
        r.headers.iter().find(|(n, _)| *n == name).map(|(_, v)| v.as_str())
    }
    fn bytes(r: &Reply) -> &[u8] {
        match &r.body {
            Body::Fixed(b) => b,
            Body::Made(b) => b,
        }
    }

    #[test]
    fn the_embedded_files_are_sorted_and_hold_a_page() {
        assert!(FILES.windows(2).all(|w| w[0].path < w[1].path), "binary search needs them sorted");
        assert!(FILES.iter().any(|f| f.path == "/index.html"));
        assert!(TEST_FILES.windows(2).all(|w| w[0].path < w[1].path));
    }

    #[test]
    fn the_page_and_its_files() {
        let s = site(&[]);
        for path in ["/", "/index.html"] {
            let r = s.answer(&get(path));
            assert_eq!(r.status, 200);
            assert_eq!(bytes(&r), b"<!doctype html><title>Hubchat</title>");
            assert_eq!(header(&r, "content-type"), Some("text/html; charset=utf-8"));
            assert_eq!(header(&r, "cache-control"), Some("no-cache"));
            assert_eq!(header(&r, "content-encoding"), None);
        }
        for (path, kind, cache) in [
            ("/assets/app-1a2b3c.js", "text/javascript; charset=utf-8", "public, max-age=31536000, immutable"),
            ("/assets/core-9f8e7d.wasm", "application/wasm", "public, max-age=31536000, immutable"),
            ("/icons/icon-192.png", "image/png", "no-cache"),
            ("/manifest.webmanifest", "application/manifest+json", "no-cache"),
            ("/sw.js", "text/javascript; charset=utf-8", "no-cache"),
        ] {
            let r = s.answer(&get(path));
            assert_eq!((r.status, header(&r, "content-type"), header(&r, "cache-control")), (200, Some(kind), Some(cache)), "{path}");
        }
    }

    #[test]
    fn compressed_by_what_the_browser_takes() {
        let s = site(&[]);
        let ask = |path: &'static str, enc: &'static str| s.answer(&Request { accept_encoding: Some(enc), ..get(path) });
        let r = ask("/index.html", "gzip, deflate, br");
        assert_eq!((bytes(&r), header(&r, "content-encoding"), header(&r, "vary")), (&b"BR-HTML"[..], Some("br"), Some("Accept-Encoding")));
        let r = ask("/index.html", "gzip, deflate");
        assert_eq!((bytes(&r), header(&r, "content-encoding")), (&b"GZ-HTML"[..], Some("gzip")));
        let r = ask("/index.html", "br;q=0, GZIP;q=0.5");
        assert_eq!((bytes(&r), header(&r, "content-encoding")), (&b"GZ-HTML"[..], Some("gzip")), "a weight of 0 refuses");
        let r = ask("/index.html", "identity");
        assert_eq!((bytes(&r), header(&r, "content-encoding"), header(&r, "vary")), (&b"<!doctype html><title>Hubchat</title>"[..], None, Some("Accept-Encoding")));
        // only what was kept: the wasm has no gzip, the picture nothing
        let r = ask("/assets/core-9f8e7d.wasm", "gzip");
        assert_eq!((bytes(&r), header(&r, "content-encoding")), (&b"\0asm-raw"[..], None));
        let r = ask("/icons/icon-192.png", "gzip, br");
        assert_eq!((bytes(&r), header(&r, "content-encoding"), header(&r, "vary")), (&b"PNG"[..], None, None));
    }

    #[test]
    fn unchanged_files_are_not_sent_again() {
        let s = site(&[]);
        let first = s.answer(&Request { accept_encoding: Some("br"), ..get("/sw.js") });
        assert_eq!(header(&first, "etag"), Some("\"00000000000000ff\""), "no Brotli kept for it: as it is");
        let first = s.answer(&Request { accept_encoding: Some("gzip"), ..get("/sw.js") });
        let tag = header(&first, "etag").unwrap().to_string();
        assert_eq!(tag, "\"00000000000000ff-gzip\"");
        for seen in [tag.as_str(), "W/\"00000000000000ff-gzip\"", "\"other\", \"00000000000000ff\"", "*"] {
            let r = s.answer(&Request { accept_encoding: Some("gzip"), if_none_match: Some(seen), ..get("/sw.js") });
            assert_eq!((r.status, r.body.len(), header(&r, "etag"), header(&r, "content-type")), (304, 0, Some(tag.as_str()), None), "{seen}");
        }
        let r = s.answer(&Request { if_none_match: Some("\"00000000000000dd\""), ..get("/sw.js") });
        assert_eq!(r.status, 200, "another file's tag");
    }

    #[test]
    fn config_follows_the_settings() {
        let read = |s: &Site| -> serde_json::Value {
            let r = s.answer(&get("/config.json"));
            assert_eq!((r.status, header(&r, "content-type"), header(&r, "cache-control")), (200, Some("application/json"), Some("no-cache")));
            serde_json::from_slice(bytes(&r)).unwrap()
        };
        assert_eq!(read(&site(&[])), serde_json::json!({ "name": "", "suggested_hubs": [], "hub_suggestions": true, "version": VERSION }));
        assert_eq!(
            read(&site(&[("HUBCHAT_WEB_NAME", "Ada's family"), ("HUBCHAT_WEB_SUGGEST_HUBS", "one.example,https://two.example:8443")])),
            serde_json::json!({ "name": "Ada's family", "suggested_hubs": ["https://one.example", "https://two.example:8443"], "hub_suggestions": true, "version": VERSION })
        );
        assert_eq!(
            read(&site(&[("HUBCHAT_WEB_HUB_SUGGESTIONS", "false"), ("HUBCHAT_WEB_SUGGEST_HUBS", "one.example")])),
            serde_json::json!({ "name": "", "suggested_hubs": [], "hub_suggestions": false, "version": VERSION })
        );
    }

    #[test]
    fn health() {
        let r = site(&[]).answer(&get("/healthz"));
        assert_eq!(r.status, 200);
        assert_eq!(serde_json::from_slice::<serde_json::Value>(bytes(&r)).unwrap(), serde_json::json!({ "ok": true, "app": "hubchat-web", "version": VERSION }));
    }

    #[test]
    fn nothing_else() {
        let s = site(&[]);
        for path in ["/nope", "/assets/", "/assets", "/index.html/", "/../etc/passwd", "/assets/../index.html", "/INDEX.HTML", "/%69ndex.html", "//index.html", "", "/api/hubs", "/ui"] {
            let r = s.answer(&get(path));
            assert_eq!((r.status, bytes(&r)), (404, &b"not found\n"[..]), "{path}");
        }
        for method in ["POST", "PUT", "DELETE", "PATCH", "OPTIONS", "CONNECT", "TRACE", "get"] {
            for path in ["/", "/config.json", "/healthz", "/nope"] {
                let r = s.answer(&Request { method, ..get(path) });
                assert_eq!((r.status, header(&r, "allow")), (405, Some("GET, HEAD")), "{method} {path}");
            }
        }
        let r = s.answer(&Request { method: "HEAD", ..get("/") });
        assert_eq!(r.status, 200);
    }

    #[test]
    fn every_answer_carries_the_safety_headers_and_never_a_cookie() {
        for (https, pairs) in [(false, vec![]), (true, vec![("HUBCHAT_WEB_TLS_CERT", "c.pem"), ("HUBCHAT_WEB_TLS_KEY", "k.pem")])] {
            let s = site(&pairs);
            for req in [get("/"), get("/assets/app-1a2b3c.js"), get("/config.json"), get("/healthz"), get("/nope"), Request { method: "POST", ..get("/") }, Request { if_none_match: Some("*"), ..get("/sw.js") }] {
                let r = s.answer(&req);
                assert_eq!(header(&r, "content-security-policy"), Some(CSP), "{}", req.path);
                assert_eq!(header(&r, "x-content-type-options"), Some("nosniff"));
                assert_eq!(header(&r, "referrer-policy"), Some("no-referrer"));
                assert_eq!(header(&r, "cross-origin-opener-policy"), Some("same-origin"));
                assert_eq!(header(&r, "permissions-policy"), Some("camera=(self), microphone=(), geolocation=()"));
                assert_eq!(header(&r, "strict-transport-security"), https.then_some("max-age=31536000"), "only with its own HTTPS");
                assert_eq!(header(&r, "set-cookie"), None);
                assert_eq!(header(&r, "access-control-allow-origin"), None);
            }
        }
    }

    /// The development server (tools/web-static.mjs) and this one send the same policy.
    #[test]
    fn the_same_policy_as_the_development_server() {
        let js = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/web-static.mjs")).unwrap();
        let start = js.find("export const CSP = ").expect("CSP in web-static.mjs");
        let end = start + js[start..].find("\";").unwrap();
        let theirs: String = js[start..end].split('"').skip(1).step_by(2).collect();
        assert_eq!(theirs, CSP);
    }
}
