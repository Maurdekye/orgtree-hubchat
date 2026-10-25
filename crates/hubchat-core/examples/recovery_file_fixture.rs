//! Synthetic file for the isolated mounted recovery proof. Never loads an account.
use hubchat_core::{recovery, Identity};

fn main() {
    let out = std::env::args().nth(1).expect("synthetic output path required");
    let me = Identity::from_parts("recovery-proof", "00112233445566778899aabbccddeeff").unwrap();
    let fixture = serde_json::json!({
        "text": recovery::to_file(&me, "2026-10-25T00:00:00Z").unwrap(),
        "words": recovery::to_words(&me).unwrap().join(" "),
    });
    std::fs::write(out, serde_json::to_vec(&fixture).unwrap()).unwrap();
}
