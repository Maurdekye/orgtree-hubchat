//! Android: Hubchat updates itself from its GitHub releases (user 2026-10-09
//! 08:29Z), reading the same latest.json as the desktop updater. Its
//! `android-aarch64` entry names the APK and carries the APK's minisign
//! signature by the updater key, which is checked here exactly as the desktop
//! updater checks the installer; Android then also insists the APK carries
//! the installed app's signing certificate. Nothing installs unless the
//! person taps Update.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

type R<T> = Result<T, String>;

/// An update the person can install.
#[derive(Serialize, Clone)]
pub struct AppUpdate {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub signature: String,
}

#[derive(Deserialize)]
struct Feed {
    version: String,
    #[serde(default)]
    notes: Option<String>,
    platforms: std::collections::HashMap<String, Platform>,
}

#[derive(Deserialize)]
struct Platform {
    url: String,
    signature: String,
}

/// Why a request for the feed or the APK failed, in plain words for the
/// banner and Settings › About.
#[cfg(target_os = "android")]
fn plain(e: reqwest::Error) -> String {
    match e.status() {
        Some(s) => format!("the release page answered {}", s.as_u16()),
        None if e.is_timeout() => "the connection timed out".into(),
        None if e.is_decode() => "the update feed can't be read".into(),
        None => "there's no connection to the release page".into(),
    }
}

/// latest.json's key for this app.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
const PLATFORM: &str = "android-aarch64";

/// The update address and key from tauri.conf.json's updater settings, the
/// ones the desktop updater uses.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn updater_config(app: &AppHandle) -> R<(String, String)> {
    let u = app.config().plugins.0.get("updater").ok_or("no update settings")?;
    let endpoint = u.get("endpoints").and_then(|e| e.get(0)).and_then(|e| e.as_str()).ok_or("no update address")?;
    let pubkey = u.get("pubkey").and_then(|k| k.as_str()).ok_or("no update key")?;
    Ok((endpoint.to_string(), pubkey.to_string()))
}

/// The newest release's Android update, if it is newer than this app.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn pick(feed: Feed, current: &semver::Version) -> R<Option<AppUpdate>> {
    let version = semver::Version::parse(feed.version.trim_start_matches('v')).map_err(|e| e.to_string())?;
    if version <= *current {
        return Ok(None);
    }
    Ok(feed.platforms.get(PLATFORM).map(|p| AppUpdate {
        version: version.to_string(),
        notes: feed.notes.clone().unwrap_or_default(),
        url: p.url.clone(),
        signature: p.signature.clone(),
    }))
}

/// The version a signature was made for: tab-separated `key:value` pairs in
/// its trusted comment (`tauri signer sign --app-version` writes `version:`).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn signed_for(trusted_comment: &str, version: &str) -> bool {
    let Some(signed) = trusted_comment.split('\t').find_map(|f| f.strip_prefix("version:")) else {
        return false;
    };
    match (semver::Version::parse(signed.trim_start_matches('v')), semver::Version::parse(version.trim_start_matches('v'))) {
        (Ok(a), Ok(b)) => a == b,
        _ => signed == version,
    }
}

/// The updater key's minisign signature over `data`, as the desktop updater
/// checks it: both are base64 of minisign's text form. Like the desktop's
/// requireSignedVersion, the signature must also name `version`, so an older
/// signed release can't be passed off as a newer one.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn verify(pubkey: &str, signature: &str, data: &[u8], version: &str) -> R<()> {
    use base64::Engine;
    let text = |b64: &str| -> R<String> {
        let raw = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|e| e.to_string())?;
        String::from_utf8(raw).map_err(|e| e.to_string())
    };
    let key = minisign_verify::PublicKey::decode(&text(pubkey)?).map_err(|e| e.to_string())?;
    let sig = text(signature).ok().and_then(|t| minisign_verify::Signature::decode(&t).ok()).ok_or("the update's signature can't be read")?;
    key.verify(data, &sig, true).map_err(|_| "the download isn't signed by Hubchat's update key".to_string())?;
    // only now is the trusted comment trustworthy: the verify above covers it
    if !signed_for(sig.trusted_comment(), version) {
        return Err(format!("the download isn't signed as Hubchat {version}"));
    }
    Ok(())
}

/// Is there a newer Hubchat for this phone? `feed` replaces the release feed
/// in test builds only (the update test reads a local one).
#[tauri::command]
pub async fn hc_app_update_check(app: AppHandle, feed: Option<String>) -> R<Option<AppUpdate>> {
    #[cfg(target_os = "android")]
    {
        let (endpoint, _) = updater_config(&app)?;
        let url = match feed {
            Some(f) if crate::android::is_test_build() => f,
            // a test build sits beside the real app; the real release would
            // install over the real one, so it reads only a test feed
            _ if crate::android::is_test_build() => return Ok(None),
            _ => endpoint,
        };
        let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().map_err(|e| e.to_string())?;
        let feed: Feed = client.get(&url).send().await.map_err(plain)?
            .error_for_status().map_err(plain)?
            .json().await.map_err(plain)?;
        pick(feed, &app.package_info().version)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, feed);
        Err("not on this platform".into())
    }
}

/// Download the update, check its signature and hand it to Android's
/// installer. Returns "permission" when Android doesn't yet let Hubchat
/// install updates (the banner explains and opens that setting), else
/// "installing". Progress goes out as `app-update-progress` [done, total].
#[tauri::command]
pub async fn hc_app_update_install(app: AppHandle, url: String, signature: String, version: String) -> R<String> {
    #[cfg(target_os = "android")]
    {
        use std::io::Write;
        use tauri::{Emitter, Manager};
        if !crate::android::can_install_updates() {
            return Ok("permission".into());
        }
        let (_, pubkey) = updater_config(&app)?;
        let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("update.apk");
        // no limit on the whole download (a slow line takes its time), but a
        // connection that stalls fails instead of hanging the banner
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .read_timeout(std::time::Duration::from_secs(60))
            .build().map_err(|e| e.to_string())?;
        let mut res = client.get(&url).send().await.map_err(plain)?.error_for_status().map_err(plain)?;
        let total = res.content_length().unwrap_or(0);
        let saved = |e: std::io::Error| format!("the update can't be saved on this phone ({e})");
        let mut file = std::fs::File::create(&path).map_err(saved)?;
        let (mut done, mut told) = (0u64, 0u64);
        while let Some(chunk) = res.chunk().await.map_err(|e| format!("the download stopped: {}", plain(e)))? {
            file.write_all(&chunk).map_err(saved)?;
            done += chunk.len() as u64;
            if done - told >= 512 * 1024 {
                told = done;
                let _ = app.emit("app-update-progress", (done, total));
            }
        }
        drop(file);
        let data = std::fs::read(&path).map_err(saved)?;
        if let Err(e) = verify(&pubkey, &signature, &data, &version) {
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }
        let _ = app.emit("app-update-progress", (done, done));
        crate::android::install_update(&path)?;
        Ok("installing".into())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, url, signature, version);
        Err("not on this platform".into())
    }
}

/// Open Android's "Install unknown apps" setting for Hubchat.
#[tauri::command]
pub fn hc_app_update_allow() {
    #[cfg(target_os = "android")]
    crate::android::open_install_settings();
}

/// The installer's last word: "", "confirm", "done" or "failed: …".
#[tauri::command]
pub fn hc_app_update_state() -> String {
    #[cfg(target_os = "android")]
    return crate::android::install_state();
    #[cfg(not(target_os = "android"))]
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(version: &str, android: bool) -> Feed {
        let mut platforms = std::collections::HashMap::new();
        platforms.insert("windows-x86_64".to_string(), Platform { url: "https://x/setup.exe".into(), signature: "w".into() });
        if android {
            platforms.insert(PLATFORM.to_string(), Platform { url: "https://x/app.apk".into(), signature: "a".into() });
        }
        Feed { version: version.into(), notes: Some("notes".into()), platforms }
    }

    #[test]
    fn only_a_newer_release_with_an_android_entry_is_offered() {
        let current = semver::Version::parse("0.1.2").unwrap();
        assert!(pick(feed("0.1.2", true), &current).unwrap().is_none());
        assert!(pick(feed("0.1.1", true), &current).unwrap().is_none());
        assert!(pick(feed("0.1.3", false), &current).unwrap().is_none());
        let u = pick(feed("v0.1.3", true), &current).unwrap().unwrap();
        assert_eq!((u.version.as_str(), u.url.as_str(), u.signature.as_str()), ("0.1.3", "https://x/app.apk", "a"));
        assert!(pick(feed("0.10.0", true), &current).unwrap().is_some());
    }

    #[test]
    fn a_bad_signature_is_refused() {
        // the real updater key, from tauri.conf.json
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let key = conf["plugins"]["updater"]["pubkey"].as_str().unwrap();
        assert!(verify(key, "bm90IGEgc2lnbmF0dXJl", b"apk", "0.1.3").is_err());
    }

    #[test]
    fn the_signature_must_name_the_announced_version() {
        let c = "timestamp:1791543797\tfile:Hubchat_0.1.3_arm64.apk\tversion:0.1.3";
        assert!(signed_for(c, "0.1.3") && signed_for(c, "v0.1.3"));
        assert!(!signed_for(c, "0.1.4"));
        assert!(!signed_for("timestamp:1791543797\tfile:Hubchat_0.1.3_arm64.apk", "0.1.3"));
    }
}
