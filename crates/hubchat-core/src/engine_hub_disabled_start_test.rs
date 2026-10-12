//! This control also compiles on the pre-toggle engine: raw saved policy and
//! the existing spawn boundary, without starting a network session.
use super::*;
struct DisabledStartHost;
impl Host for DisabledStartHost {
    fn event(&self, _: Event) {}
    fn open_source(&self, _: &str) -> std::io::Result<std::fs::File> { unreachable!() }
    fn download_dir(&self) -> PathBuf { PathBuf::new() }
}
#[tokio::test]
async fn saved_disabled_policy_is_respected_before_spawning() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let url = "http://127.0.0.1:7997";
    store.add_hub(url, "t0").unwrap();
    store.set_meta(&format!("hub.disabled.{url}"), "yes").unwrap();
    let e = Engine::new(store, Identity::generate("disabled-start").unwrap(), Profile::default(), Arc::new(DisabledStartHost));
    e.set_suspended(true);
    e.spawn_hub(HubAddress::parse(url).unwrap(), "saved".into(), None);
    let admitted = e.hubs.lock().unwrap().contains_key(url);
    e.shutdown();
    tokio::task::yield_now().await;
    assert!(!admitted, "disabled saved hub must not admit a runtime or connection task");
}
