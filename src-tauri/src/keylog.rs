//! A short record of what happened to the identity key on this device: found,
//! restored, missing, saved, forgotten (user 2026-10-10: after Windows lost
//! every saved sign-in, nothing showed what Hubchat had found). Never the key
//! and never an address. A text file in Hubchat's folder, newest last, kept
//! under 64 KB.

use std::io::Write;
use std::path::Path;

const FILE: &str = "identity-log.txt";
const MAX: usize = 64 * 1024;

pub fn note(dir: &Path, what: &str) {
    let path = dir.join(FILE);
    // over the limit: keep the newer half, from a line start
    if let Ok(s) = std::fs::read_to_string(&path) {
        if s.len() > MAX {
            let half = (s.len() / 2..s.len()).find(|&i| s.is_char_boundary(i)).unwrap_or(s.len());
            let start = s[half..].find('\n').map_or(s.len(), |i| half + i + 1);
            let _ = std::fs::write(&path, &s[start..]);
        }
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} {what}", hubchat_core::engine::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_go_to_the_end_and_the_file_stays_under_the_limit() {
        let dir = std::env::temp_dir().join(format!("hubchat-keylog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        note(&dir, "key found in Credential Manager");
        note(&dir, "no key in Credential Manager; no backup");
        let s = std::fs::read_to_string(dir.join(FILE)).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with(" no key in Credential Manager; no backup"), "{s}");
        for _ in 0..3000 {
            note(&dir, "key found in Credential Manager");
        }
        let s = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(s.len() <= MAX + 200, "{} bytes", s.len());
        assert!(s.lines().all(|l| l.ends_with("Credential Manager")), "whole lines only");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
