//! A short record of what happened to the identity key on this device: found,
//! restored, missing, saved, forgotten (user 2026-10-10: after Windows lost
//! every saved sign-in, nothing showed what Hubchat had found). Never the key
//! and never an address. A text file in Hubchat's folder, newest last, kept
//! under 64 KB.
//!
//! The normal log (trace.rs, docs/logging.md) gets a mirror of each event, but
//! only as `Fact`s: a fixed phrase per fact, never the line written to this
//! file. That line may carry the key store's or the backup's own error text,
//! which the normal log's privacy rules keep out (kinds, not text).

use std::io::Write;
use std::path::Path;

const FILE: &str = "identity-log.txt";
const MAX: usize = 64 * 1024;

/// What the normal log may say about the key. Add a variant, not a format
/// string: the phrase is the whole mirror.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fact {
    KeyFound,
    /// The main copy has no usable key (missing or damaged).
    StoreHasNoKey,
    /// The key store did not answer; it may still hold the key.
    StoreDidNotAnswer,
    BackupWritten,
    BackupNotWritten,
    RestoredFromBackup,
    PutBackInStore,
    StoreRefusedItBack,
    BackupDamaged,
    NoBackup,
    BackupUnreadable,
    #[cfg_attr(not(target_os = "android"), allow(dead_code))] // only Android has push cleanup
    PushCleanupFailed,
    StartedOver,
    LeftIdentity,
    OtherLocalDataRemoved,
    KeySavedNew,
    KeySavedBrought,
}

impl Fact {
    pub fn phrase(self) -> &'static str {
        match self {
            Fact::KeyFound => "key found in the key store",
            Fact::StoreHasNoKey => "the key store has no usable key",
            Fact::StoreDidNotAnswer => "the key store did not answer",
            Fact::BackupWritten => "backup written",
            Fact::BackupNotWritten => "backup could not be written",
            Fact::RestoredFromBackup => "key restored from the backup",
            Fact::PutBackInStore => "put back into the key store",
            Fact::StoreRefusedItBack => "the key store refused it back",
            Fact::BackupDamaged => "the backup is damaged",
            Fact::NoBackup => "no backup",
            Fact::BackupUnreadable => "the backup did not open",
            Fact::PushCleanupFailed => "push cleanup failed while discarding the identity",
            Fact::StartedOver => "started over: local data of the lost identity removed",
            Fact::LeftIdentity => "left this identity: key and local data forgotten",
            Fact::OtherLocalDataRemoved => "another identity's local data removed before taking this one",
            Fact::KeySavedNew => "key saved: a new identity",
            Fact::KeySavedBrought => "key saved: an identity brought to this device",
        }
    }
}

/// Add `what` to the file, and `facts` to the normal log.
pub fn note(dir: &Path, what: &str, facts: &[Fact]) {
    let mirror: Vec<&str> = facts.iter().map(|f| f.phrase()).collect();
    tracing::info!(target: "keylog", "identity key: {}", mirror.join("; "));
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
        note(&dir, "key found in Credential Manager", &[Fact::KeyFound]);
        note(&dir, "no key in Credential Manager; no backup", &[Fact::StoreHasNoKey, Fact::NoBackup]);
        let s = std::fs::read_to_string(dir.join(FILE)).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with(" no key in Credential Manager; no backup"), "{s}");
        for _ in 0..3000 {
            note(&dir, "key found in Credential Manager", &[Fact::KeyFound]);
        }
        let s = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(s.len() <= MAX + 200, "{} bytes", s.len());
        assert!(s.lines().all(|l| l.ends_with("Credential Manager")), "whole lines only");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
