//! The built `hubchat-web` program, started on this machine's loopback and
//! asked over real sockets (spec R6.1, R6.2; test plan "Server and image").
//! It works with whatever page the program holds: the stand-in, or a built
//! Hubchat Web (HUBCHAT_WEB_DIST).
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const EXE: &str = env!("CARGO_BIN_EXE_hubchat-web");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; media-src blob:; connect-src https: http://localhost:* http://127.0.0.1:*; worker-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'";

/// The program with only the settings a test gives it.
fn program(env: &[(&str, &str)]) -> Command {
    let mut c = Command::new(EXE);
    for (name, _) in std::env::vars().filter(|(n, _)| n.starts_with("HUBCHAT_WEB_")) {
        c.env_remove(name);
    }
    c.envs(env.iter().copied());
    c
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

struct Server {
    child: Child,
    port: u16,
}

impl Server {
    fn start(env: &[(&str, &str)]) -> Server {
        let port = free_port();
        let port_text = port.to_string();
        let mut all = vec![("HUBCHAT_WEB_BIND", "127.0.0.1"), ("HUBCHAT_WEB_PORT", port_text.as_str())];
        all.extend_from_slice(env);
        let mut child = program(&all).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("start hubchat-web");
        let until = Instant::now() + Duration::from_secs(20);
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            if let Some(status) = child.try_wait().unwrap() {
                let mut err = String::new();
                child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
                panic!("hubchat-web ended at once ({status}): {err}");
            }
            assert!(Instant::now() < until, "hubchat-web did not listen on {port} within 20 s");
            std::thread::sleep(Duration::from_millis(50));
        }
        Server { child, port }
    }

    /// Stops the server and gives what it wrote: (stdout, stderr).
    fn stop(mut self) -> (String, String) {
        self.child.kill().unwrap();
        let (mut out, mut err) = (String::new(), String::new());
        self.child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
        self.child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
        self.child.wait().unwrap();
        (out, err)
    }

    fn ask(&self, method: &str, path: &str, headers: &[(&str, &str)]) -> Answer {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut req = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
        for (n, v) in headers {
            req += &format!("{n}: {v}\r\n");
        }
        s.write_all((req + "\r\n").as_bytes()).unwrap();
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).unwrap();
        let cut = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap_or_else(|| panic!("no answer to {method} {path}: {:?}", String::from_utf8_lossy(&raw)));
        let head = String::from_utf8(raw[..cut].to_vec()).unwrap();
        let mut lines = head.split("\r\n");
        let status: u16 = lines.next().unwrap().split(' ').nth(1).unwrap().parse().unwrap();
        let headers = lines.map(|l| l.split_once(':').unwrap()).map(|(n, v)| (n.to_ascii_lowercase(), v.trim().to_string())).collect();
        Answer { status, headers, body: raw[cut + 4..].to_vec() }
    }

    fn get(&self, path: &str) -> Answer {
        self.ask("GET", path, &[])
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Answer {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| panic!("not JSON ({e}): {:?}", String::from_utf8_lossy(&self.body)))
    }
}

fn run(args: &[&str], env: &[(&str, &str)]) -> Output {
    program(env).args(args).output().expect("run hubchat-web")
}

/// The paths of the files inside the program (`hubchat-web files`).
fn files() -> Vec<String> {
    let out = run(&["files"], &[]);
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap().lines().map(|l| l.split_whitespace().last().unwrap().to_string()).collect()
}

#[test]
fn version_names_the_program_and_its_version() {
    let out = run(&["version"], &[]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with(&format!("hubchat-web {VERSION}")), "{text}");
    // a build without the real page says so; one with it says nothing more
    assert_eq!(text.trim() == format!("hubchat-web {VERSION}"), option_env!("HUBCHAT_WEB_DIST").is_some_and(|d| !d.trim().is_empty()), "{text}");
    let bad = run(&["frobnicate"], &[]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).starts_with("usage: hubchat-web"));
}

#[test]
fn every_path_and_its_headers() {
    let server = Server::start(&[]);
    let files = files();
    assert!(files.contains(&"/index.html".to_string()) && files.contains(&"/sw.js".to_string()) && files.contains(&"/manifest.webmanifest".to_string()), "{files:?}");
    let asset = files.iter().find(|f| f.starts_with("/assets/") && f.ends_with(".js")).expect("a script under /assets/");

    // the page, at / and by its name
    let page = server.get("/");
    assert_eq!((page.status, page.header("content-type"), page.header("cache-control")), (200, Some("text/html; charset=utf-8"), Some("no-cache")));
    assert!(String::from_utf8_lossy(&page.body).to_ascii_lowercase().starts_with("<!doctype html>"));
    assert_eq!(page.header("content-length"), Some(page.body.len().to_string().as_str()));
    let named = server.get("/index.html?from=home");
    assert_eq!((named.status, &named.body), (200, &page.body), "a query changes nothing");

    // a file with a hash in its name is kept by the browser for good
    let script = server.get(asset);
    assert_eq!((script.status, script.header("content-type"), script.header("cache-control")), (200, Some("text/javascript; charset=utf-8"), Some("public, max-age=31536000, immutable")));
    assert_eq!(script.header("content-encoding"), None, "not asked for, not compressed");
    for (accept, coding) in [("gzip, deflate, br", "br"), ("gzip", "gzip")] {
        let packed = server.ask("GET", asset, &[("Accept-Encoding", accept)]);
        assert_eq!((packed.status, packed.header("content-encoding"), packed.header("vary")), (200, Some(coding), Some("Accept-Encoding")));
        assert!(packed.body.len() < script.body.len(), "{coding}: {} B against {} B", packed.body.len(), script.body.len());
        assert_eq!(packed.header("content-length"), Some(packed.body.len().to_string().as_str()));
    }
    let gz = server.ask("GET", asset, &[("Accept-Encoding", "gzip")]);
    assert_eq!(&gz.body[..2], &[0x1f, 0x8b], "gzip's own first bytes");

    // asked again with what the browser holds: not sent again
    for path in ["/sw.js", "/manifest.webmanifest"] {
        let first = server.get(path);
        assert_eq!((first.status, first.header("cache-control")), (200, Some("no-cache")), "{path}");
        let tag = first.header("etag").expect("an ETag");
        let again = server.ask("GET", path, &[("If-None-Match", tag)]);
        assert_eq!((again.status, again.body.len(), again.header("etag")), (304, 0, Some(tag)), "{path}");
    }
    assert_eq!(server.get("/sw.js").header("content-type"), Some("text/javascript; charset=utf-8"));
    assert_eq!(server.get("/manifest.webmanifest").header("content-type"), Some("application/manifest+json"));

    // HEAD: the same headers, the GET's length, no body
    let head = server.ask("HEAD", "/", &[]);
    assert_eq!((head.status, head.body.len(), head.header("content-length"), head.header("content-type")), (200, 0, page.header("content-length"), Some("text/html; charset=utf-8")));

    // the two answers the server makes itself
    let health = server.get("/healthz");
    assert_eq!((health.status, health.header("content-type")), (200, Some("application/json")));
    assert_eq!(health.json(), serde_json::json!({ "ok": true, "app": "hubchat-web", "version": VERSION }));
    let config = server.get("/config.json");
    assert_eq!((config.status, config.header("content-type"), config.header("cache-control")), (200, Some("application/json"), Some("no-cache")));
    assert_eq!(config.json(), serde_json::json!({ "name": "", "suggested_hubs": [], "hub_suggestions": true, "version": VERSION }));

    // nothing else, and no other method
    for path in ["/nope", "/assets/", "/assets/../index.html", "/..%2f..%2fetc/passwd", "/api/hubs", "/ui", "/.env", "/index.html/"] {
        assert_eq!(server.get(path).status, 404, "{path}");
    }
    for method in ["POST", "PUT", "DELETE", "PATCH", "OPTIONS"] {
        for path in ["/", "/config.json", "/healthz"] {
            let r = server.ask(method, path, &[("Content-Length", "0")]);
            assert_eq!((r.status, r.header("allow")), (405, Some("GET, HEAD")), "{method} {path}");
        }
    }
    // a web page elsewhere asking first (CORS): refused like any other method, and never allowed
    let pre = server.ask("OPTIONS", "/config.json", &[("Origin", "https://elsewhere.example"), ("Access-Control-Request-Method", "GET")]);
    assert_eq!((pre.status, pre.header("access-control-allow-origin")), (405, None));

    // every answer: the safety headers, never a cookie, never HSTS over plain HTTP
    for r in [&page, &script, &health, &config, &head, &pre, &server.get("/nope"), &server.ask("GET", "/sw.js", &[("If-None-Match", "*")])] {
        assert_eq!(r.header("content-security-policy"), Some(CSP));
        assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
        assert_eq!(r.header("referrer-policy"), Some("no-referrer"));
        assert_eq!(r.header("cross-origin-opener-policy"), Some("same-origin"));
        assert_eq!(r.header("permissions-policy"), Some("camera=(self), microphone=(), geolocation=()"));
        assert_eq!(r.header("set-cookie"), None);
        assert_eq!(r.header("strict-transport-security"), None);
    }
}

#[test]
fn config_follows_the_settings() {
    let server = Server::start(&[("HUBCHAT_WEB_NAME", "Ada's family"), ("HUBCHAT_WEB_SUGGEST_HUBS", "Hub.Example, https://two.example:8443/,hub.example")]);
    assert_eq!(
        server.get("/config.json").json(),
        serde_json::json!({ "name": "Ada's family", "suggested_hubs": ["https://hub.example", "https://two.example:8443"], "hub_suggestions": true, "version": VERSION })
    );
    let (_, err) = server.stop();
    assert!(err.contains("suggested hubs: https://hub.example, https://two.example:8443"), "{err}");

    // off wins over a list, and the server says the list is ignored
    let server = Server::start(&[("HUBCHAT_WEB_HUB_SUGGESTIONS", "false"), ("HUBCHAT_WEB_SUGGEST_HUBS", "hub.example")]);
    assert_eq!(server.get("/config.json").json(), serde_json::json!({ "name": "", "suggested_hubs": [], "hub_suggestions": false, "version": VERSION }));
    let (_, err) = server.stop();
    assert!(err.contains("HUBCHAT_WEB_HUB_SUGGESTIONS is false: HUBCHAT_WEB_SUGGEST_HUBS is ignored, no hub is suggested."), "{err}");
    assert!(err.contains("no hub is suggested)"), "{err}");
}

#[test]
fn a_bad_setting_refuses_to_start() {
    for (env, says) in [
        (vec![("HUBCHAT_WEB_PORT", "eighty")], "HUBCHAT_WEB_PORT=\"eighty\": write a port from 1 to 65535."),
        (vec![("HUBCHAT_WEB_BIND", "everywhere")], "HUBCHAT_WEB_BIND=\"everywhere\": write an IP address"),
        (vec![("HUBCHAT_WEB_HUB_SUGGESTIONS", "nope")], "HUBCHAT_WEB_HUB_SUGGESTIONS=\"nope\": write true or false."),
        (vec![("HUBCHAT_WEB_SUGGEST_HUBS", "hub.example,http://far.example")], "HUBCHAT_WEB_SUGGEST_HUBS: \"http://far.example\" is not a hub address"),
        (vec![("HUBCHAT_WEB_SUGGEST_HUBS", "a.x,b.x,c.x,d.x,e.x,f.x,g.x,h.x,i.x")], "HUBCHAT_WEB_SUGGEST_HUBS lists 9 hubs: at most 8."),
        (vec![("HUBCHAT_WEB_NAME", "a name that is much longer than the forty-eight characters allowed")], "HUBCHAT_WEB_NAME is 66 characters long: at most 48."),
        (vec![("HUBCHAT_WEB_LOG", "everything")], "HUBCHAT_WEB_LOG=\"everything\": write requests or off."),
        (vec![("HUBCHAT_WEB_TLS_CERT", "cert.pem")], "HUBCHAT_WEB_TLS_CERT is set but HUBCHAT_WEB_TLS_KEY is not"),
        (vec![("HUBCHAT_WEB_TLS_KEY", "key.pem")], "HUBCHAT_WEB_TLS_KEY is set but HUBCHAT_WEB_TLS_CERT is not"),
    ] {
        for args in [&[][..], &["healthcheck"][..]] {
            let out = run(args, &env);
            let err = String::from_utf8_lossy(&out.stderr);
            assert_eq!(out.status.code(), Some(2), "{env:?} {args:?}: {err}");
            assert!(err.starts_with(&format!("hubchat-web: {says}")), "{env:?}: {err}");
        }
    }
    // files that are not there, or are not a certificate
    let port = free_port().to_string();
    let here = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/tls");
    let out = run(&[], &[("HUBCHAT_WEB_BIND", "127.0.0.1"), ("HUBCHAT_WEB_PORT", &port), ("HUBCHAT_WEB_TLS_CERT", &format!("{here}/missing.pem")), ("HUBCHAT_WEB_TLS_KEY", &format!("{here}/test-key.pem"))]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("HUBCHAT_WEB_TLS_CERT: can't read"), "{}", String::from_utf8_lossy(&out.stderr));
    let out = run(&[], &[("HUBCHAT_WEB_BIND", "127.0.0.1"), ("HUBCHAT_WEB_PORT", &port), ("HUBCHAT_WEB_TLS_CERT", &format!("{here}/test-cert.pem")), ("HUBCHAT_WEB_TLS_KEY", &format!("{here}/test-cert.pem"))]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("HUBCHAT_WEB_TLS_KEY: can't read a private key"), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn a_taken_port_says_so() {
    let holder = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = holder.local_addr().unwrap().port().to_string();
    let out = run(&[], &[("HUBCHAT_WEB_BIND", "127.0.0.1"), ("HUBCHAT_WEB_PORT", &port)]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains(&format!("can't listen on 127.0.0.1:{port}")), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn healthcheck_says_whether_the_server_answers() {
    let server = Server::start(&[]);
    let port = server.port.to_string();
    // the container runs it with the server's own settings; 0.0.0.0 is asked on loopback
    for bind in ["127.0.0.1", "0.0.0.0"] {
        let ok = run(&["healthcheck"], &[("HUBCHAT_WEB_BIND", bind), ("HUBCHAT_WEB_PORT", &port)]);
        assert!(ok.status.success(), "{bind}: {}", String::from_utf8_lossy(&ok.stderr));
    }
    drop(server);
    let gone = run(&["healthcheck"], &[("HUBCHAT_WEB_BIND", "127.0.0.1"), ("HUBCHAT_WEB_PORT", &port)]);
    assert_eq!(gone.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&gone.stderr).starts_with("hubchat-web healthcheck: no answer on 127.0.0.1:"), "{}", String::from_utf8_lossy(&gone.stderr));

    // something else on that port is not Hubchat Web
    let other = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = other.local_addr().unwrap().port().to_string();
    let answering = std::thread::spawn(move || {
        let (mut s, _) = other.accept().unwrap();
        let mut buf = [0u8; 512];
        let _ = s.read(&mut buf);
        s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").unwrap();
    });
    let wrong = run(&["healthcheck"], &[("HUBCHAT_WEB_BIND", "127.0.0.1"), ("HUBCHAT_WEB_PORT", &port)]);
    answering.join().unwrap();
    assert_eq!(wrong.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("answered, but not as Hubchat Web"), "{}", String::from_utf8_lossy(&wrong.stderr));
}

#[test]
fn with_a_certificate_and_key_it_speaks_https() {
    let here = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/tls");
    let (cert, key) = (format!("{here}/test-cert.pem"), format!("{here}/test-key.pem"));
    let tls = [("HUBCHAT_WEB_TLS_CERT", cert.as_str()), ("HUBCHAT_WEB_TLS_KEY", key.as_str())];
    let server = Server::start(&tls);
    let port = server.port.to_string();
    // the health check speaks TLS to it, trusting exactly that certificate
    let mut env = vec![("HUBCHAT_WEB_BIND", "127.0.0.1"), ("HUBCHAT_WEB_PORT", port.as_str())];
    env.extend_from_slice(&tls);
    let ok = run(&["healthcheck"], &env);
    assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
    // plain HTTP to it gets no page
    let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
    let mut raw = Vec::new();
    let _ = s.read_to_end(&mut raw);
    assert!(!String::from_utf8_lossy(&raw).contains("200 OK"), "{:?}", String::from_utf8_lossy(&raw));
    // and a health check that expects plain HTTP does not pass
    let plain = run(&["healthcheck"], &[("HUBCHAT_WEB_BIND", "127.0.0.1"), ("HUBCHAT_WEB_PORT", &port)]);
    assert_eq!(plain.status.code(), Some(1));
    let (_, err) = server.stop();
    assert!(err.contains(&format!("listening on https://127.0.0.1:{port}")), "{err}");
}

#[test]
fn one_log_line_per_request_and_nothing_about_who_asked() {
    let server = Server::start(&[]);
    server.ask("GET", "/?secret=words", &[("Cookie", "id=someone"), ("User-Agent", "a-browser")]);
    server.get("/nope");
    server.ask("POST", "/config.json", &[("Content-Length", "0")]);
    let (out, _) = server.stop();
    let lines: Vec<serde_json::Value> = out.lines().map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not a JSON line ({e}): {l}"))).collect();
    let seen: Vec<(&str, u64)> = lines.iter().map(|l| (l["path"].as_str().unwrap(), l["status"].as_u64().unwrap())).collect();
    assert_eq!(seen, [("/", 200), ("/nope", 404), ("/config.json", 405)]);
    for l in &lines {
        let mut keys: Vec<&str> = l.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["ms", "path", "status", "t"]);
        let t = l["t"].as_str().unwrap();
        assert!(t.len() == 20 && t.ends_with('Z') && t.as_bytes()[10] == b'T', "{t}");
    }
    assert!(!out.contains("secret") && !out.contains("someone") && !out.contains("a-browser") && !out.contains("127.0.0.1"), "{out}");

    let quiet = Server::start(&[("HUBCHAT_WEB_LOG", "off")]);
    quiet.get("/");
    let (out, _) = quiet.stop();
    assert_eq!(out, "", "HUBCHAT_WEB_LOG=off");
}
