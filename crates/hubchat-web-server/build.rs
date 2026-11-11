// Packs the built web app into the program (spec R6.1: "with the built web
// app compiled in"), each file as it is and, where that is smaller, also as
// Brotli and gzip, so the server never compresses while it runs.
//
// HUBCHAT_WEB_DIST names the built page (tools/web-build.mjs, `dist-web`).
// Without it the small stand-in page beside this file goes in, so the
// crate builds and tests anywhere; such a build says so in `hubchat-web
// version`, and the release's smoke test refuses it.
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, root: &Path, into: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = fs::read_dir(dir).unwrap_or_else(|e| panic!("can't read {}: {e}", dir.display())).map(|e| e.unwrap().path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, root, into);
        } else {
            let rel = p.strip_prefix(root).unwrap().components().map(|c| c.as_os_str().to_str().expect("a file name that is not UTF-8")).collect::<Vec<_>>().join("/");
            into.push((format!("/{rel}"), p));
        }
    }
}

/// FNV-1a, 64 bit: names a file's content in its ETag.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
}

fn compressible(path: &str) -> bool {
    matches!(path.rsplit('.').next().unwrap_or(""), "html" | "js" | "css" | "wasm" | "json" | "svg" | "webmanifest" | "txt" | "map")
}

fn main() {
    println!("cargo:rerun-if-env-changed=HUBCHAT_WEB_DIST");
    println!("cargo:rustc-check-cfg=cfg(stand_in)");
    let here = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let dist = match std::env::var("HUBCHAT_WEB_DIST") {
        Ok(d) if !d.trim().is_empty() => PathBuf::from(d),
        _ => {
            println!("cargo:rustc-cfg=stand_in");
            here.join("stand-in")
        }
    };
    println!("cargo:rerun-if-changed={}", dist.display());
    assert!(dist.join("index.html").is_file(), "HUBCHAT_WEB_DIST: no index.html in {} (build the page first: npm run build:web)", dist.display());

    let mut files = Vec::new();
    walk(&dist, &dist, &mut files);
    files.sort();
    let mut table = String::from("pub static FILES: &[File] = &[\n");
    for (n, (path, file)) in files.iter().enumerate() {
        let raw = fs::read(file).unwrap();
        fs::write(out.join(format!("{n}.raw")), &raw).unwrap();
        let mut br = None;
        let mut gz = None;
        if compressible(path) {
            let mut packed = Vec::new();
            let params = brotli::enc::BrotliEncoderParams { quality: 11, lgwin: 22, ..Default::default() };
            brotli::BrotliCompress(&mut &raw[..], &mut packed, &params).unwrap();
            if packed.len() < raw.len() {
                fs::write(out.join(format!("{n}.br")), &packed).unwrap();
                br = Some(n);
            }
            let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
            enc.write_all(&raw).unwrap();
            let packed = enc.finish().unwrap();
            if packed.len() < raw.len() {
                fs::write(out.join(format!("{n}.gz")), &packed).unwrap();
                gz = Some(n);
            }
        }
        let inc = |n: &Option<usize>, ext: &str| match n {
            Some(n) => format!("Some(include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{n}.{ext}\")))"),
            None => "None".into(),
        };
        writeln!(
            table,
            "    File {{ path: {path:?}, raw: include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{n}.raw\")), br: {}, gz: {}, tag: \"{:016x}\" }},",
            inc(&br, "br"),
            inc(&gz, "gz"),
            fnv(&raw)
        )
        .unwrap();
    }
    table.push_str("];\n");
    fs::write(out.join("files.rs"), table).unwrap();
}
