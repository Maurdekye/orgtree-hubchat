// Compile with the locked build's tauri_utils + serde_json rlibs. This probes
// the actual pinned parser, not a JavaScript approximation or copied enum.
fn main() {
    use tauri_utils::config::FrontendDist;
    let bad: FrontendDist = serde_json::from_str(r#""C:/build/frontend""#).unwrap();
    assert!(matches!(bad, FrontendDist::Url(_)), "bad drive path must reproduce URL interpretation");
    let file = std::env::args().nth(1).expect("config path");
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    let actual: FrontendDist = serde_json::from_value(config["build"]["frontendDist"].clone()).unwrap();
    let FrontendDist::Directory(directory) = actual else { panic!("packaging config is not a Directory") };
    assert!(directory.join("index.html").is_file());
    assert!(directory.join("assets").is_dir());
    println!("PINNED-PARSER-OK: bad drive path=Url; corrected config=Directory; index/assets exist");
}
