//! Which certificates a hub connection trusts.
//!
//! Not written yet: these are the names `tests/tls_trust.rs` needs, doing
//! nothing, so that the tests fail for what the apps do today (a certificate
//! from a certificate authority of the hub's own is refused whatever the
//! device trusts).

/// Where the device's trusted root certificates come from: each as DER
/// bytes, or `None` when they can't be read now.
pub fn set_device_roots(_source: impl Fn() -> Option<Vec<Vec<u8>>> + Send + Sync + 'static) {}

/// Back to the machine's own certificate store.
pub fn reset_device_roots() {}
