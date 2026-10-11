//! Where Hubchat keeps what it records about itself on this device: the
//! `diagnostics` folder in its app data folder. Nothing in it is ever sent
//! anywhere.

use std::path::{Path, PathBuf};

pub fn root(app_data: &Path) -> PathBuf {
    app_data.join("diagnostics")
}

/// Panic reports (crash.rs).
pub fn crash_reports(app_data: &Path) -> PathBuf {
    root(app_data).join("crash-reports")
}
