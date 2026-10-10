//! JNI bridge for ConnectionService.kt and SecretBox.kt. The service calls
//! `startCore` when it is created (also after a START_STICKY restart with no
//! activity); the core calls back into static Kotlin methods to post
//! notifications and to seal/open secrets with the Android Keystore.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};

struct Jni {
    vm: JavaVM,
    // Classes must be resolved on a Java thread: FindClass from the core's
    // own threads only sees the system class loader.
    service: GlobalRef,
    secret_box: GlobalRef,
}

static JNI: OnceLock<Jni> = OnceLock::new();

/// Call `static String|void <method>(String...)` on one of our classes.
fn call(class: &GlobalRef, method: &str, args: &[&str], returns_string: bool) -> Option<String> {
    let jni = JNI.get()?;
    let mut env = jni.vm.attach_current_thread_as_daemon().ok()?;
    let strings: Vec<_> = args.iter().filter_map(|a| env.new_string(a).ok()).collect();
    if strings.len() != args.len() {
        return None;
    }
    let values: Vec<JValue> = strings.iter().map(|s| JValue::Object(s.as_ref())).collect();
    let ret = if returns_string {
        "Ljava/lang/String;"
    } else {
        "V"
    };
    let sig = format!("({}){ret}", "Ljava/lang/String;".repeat(args.len()));
    let class: &JClass = class.as_obj().into();
    match env.call_static_method(class, method, &sig, &values) {
        Ok(v) if returns_string => {
            let obj = v.l().ok()?;
            let s: String = env.get_string(&JString::from(obj)).ok()?.into();
            Some(s)
        }
        Ok(_) => Some(String::new()),
        Err(_) => {
            let _ = env.exception_clear();
            None
        }
    }
}

/// In-app updates: may Hubchat install its own updates?
pub fn can_install_updates() -> bool {
    JNI.get().and_then(|j| call(&j.service, "canInstallUpdates", &[], true)).as_deref() == Some("1")
}

/// Open Settings › Install unknown apps for Hubchat.
pub fn open_install_settings() {
    if let Some(j) = JNI.get() {
        call(&j.service, "openInstallSettings", &[], false);
    }
}

/// Hand a verified APK to Android's installer.
pub fn install_update(path: &std::path::Path) -> Result<(), String> {
    let j = JNI.get().ok_or("Hubchat isn't ready yet")?;
    match call(&j.service, "installUpdate", &[&path.to_string_lossy()], true) {
        Some(e) if e.is_empty() => Ok(()),
        Some(e) => Err(e),
        None => Err("Android's installer didn't answer".into()),
    }
}

/// The installer's last word: "", "confirm", "done" or "failed: …".
pub fn install_state() -> String {
    JNI.get().and_then(|j| call(&j.service, "installState", &[], true)).unwrap_or_default()
}

/// A test build (package *.test), which may read a local update feed.
pub fn is_test_build() -> bool {
    JNI.get().and_then(|j| call(&j.service, "isTestBuild", &[], true)).as_deref() == Some("1")
}

/// Seal a secret with the Keystore-held key. None if the Keystore failed.
pub fn seal(plain: &str) -> Option<String> {
    call(&JNI.get()?.secret_box, "seal", &[plain], true).filter(|s| !s.is_empty())
}

pub fn open(sealed: &str) -> Option<String> {
    call(&JNI.get()?.secret_box, "open", &[sealed], true).filter(|s| !s.is_empty())
}

/// `static int openFd(String uri)` on ConnectionService: a content:// URI
/// (or path) opened read-only through the ContentResolver; -1 on failure.
fn open_fd(source: &str) -> Option<i32> {
    let jni = JNI.get()?;
    let mut env = jni.vm.attach_current_thread_as_daemon().ok()?;
    let arg = env.new_string(source).ok()?;
    let class: &JClass = jni.service.as_obj().into();
    match env.call_static_method(
        class,
        "openFd",
        "(Ljava/lang/String;)I",
        &[JValue::Object(arg.as_ref())],
    ) {
        Ok(v) => v.i().ok().filter(|fd| *fd >= 0),
        Err(_) => {
            let _ = env.exception_clear();
            None
        }
    }
}

struct AndroidPlatform {
    dir: PathBuf,
}

impl crate::core::Platform for AndroidPlatform {
    fn app_installed(&self, package: &str) -> Option<bool> {
        let j = JNI.get()?;
        match call(&j.service, "isInstalled", &[package], true)?.as_str() {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        }
    }
    fn open_app(&self, what: &str) -> bool {
        JNI.get()
            .and_then(|j| call(&j.service, "openApp", &[what], true))
            .is_some_and(|r| r == "1")
    }
    fn vpn_active(&self) -> Option<bool> {
        let j = JNI.get()?;
        match call(&j.service, "vpnActive", &[], true)?.as_str() {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        }
    }
    /// The sound follows Android's own settings for the Messages channel
    /// (the design has no sound switch on Android).
    fn notify(&self, title: &str, body: &str, peer: &str, _sound: bool) {
        if let Some(j) = JNI.get() {
            call(&j.service, "notifyMessage", &[title, body, peer], false);
        }
    }
    fn status(&self, text: &str) {
        if let Some(j) = JNI.get() {
            call(&j.service, "setStatus", &[text], false);
        }
    }
    fn open_source(&self, source: &str) -> std::io::Result<std::fs::File> {
        if !source.starts_with("content://") {
            return std::fs::File::open(source);
        }
        let fd =
            open_fd(source).ok_or_else(|| std::io::Error::other("can't open the picked file"))?;
        // SAFETY: openFd hands us a detached descriptor we now own.
        Ok(unsafe {
            use std::os::fd::FromRawFd;
            std::fs::File::from_raw_fd(fd)
        })
    }
    fn source_name(&self, source: &str) -> Option<String> {
        if !source.starts_with("content://") {
            return std::path::Path::new(source)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
        }
        let j = JNI.get()?;
        call(&j.service, "displayName", &[source], true).filter(|n| !n.is_empty())
    }
    fn publish_download(&self, path: &std::path::Path, name: &str) -> Option<String> {
        let j = JNI.get()?;
        call(
            &j.service,
            "publishDownload",
            &[&path.to_string_lossy(), name],
            true,
        )
        .filter(|u| !u.is_empty())
    }
    fn take_pending_link(&self) -> Option<String> {
        let j = JNI.get()?;
        call(&j.service, "takePendingLink", &[], true).filter(|p| !p.is_empty())
    }
    fn take_pending_chat(&self) -> Option<String> {
        let j = JNI.get()?;
        call(&j.service, "takePendingPeer", &[], true).filter(|p| !p.is_empty())
    }
    fn download_dir(&self) -> PathBuf {
        self.dir.join("downloads")
    }
    fn stay_connected(&self) -> Option<bool> {
        let j = JNI.get()?;
        call(&j.service, "isStayConnected", &[], true).map(|v| v != "0")
    }
    fn set_stay_connected(&self, on: bool) -> Result<(), String> {
        let j = JNI.get().ok_or("Hubchat is still starting")?;
        call(&j.service, "setStayConnected", &[if on { "1" } else { "0" }], false)
            .map(|_| ())
            .ok_or_else(|| "Android refused the change".to_string())
    }
    fn push_state(&self) -> Option<crate::core::PushState> {
        let j = JNI.get()?;
        serde_json::from_str(&call(&j.service, "pushState", &[], true)?).ok()
    }
    fn push_active(&self) -> bool {
        JNI.get().and_then(|j| call(&j.service, "isPushActive", &[], true))
            .is_some_and(|value| value == "1")
    }
    fn set_push(&self, on: bool, distributor: &str) -> Result<(), String> {
        let j = JNI.get().ok_or("Hubchat is still starting")?;
        match call(&j.service, "setPush", &[if on { "1" } else { "0" }, distributor], true) {
            Some(e) if e.is_empty() => Ok(()),
            Some(e) => Err(e),
            None => Err("Android could not change push settings".into()),
        }
    }
    fn refresh_push(&self) {
        if let Some(j) = JNI.get() { call(&j.service, "refreshPush", &[], false); }
    }
    fn clear_notification(&self, peer: &str) {
        if let Some(j) = JNI.get() {
            let _ = call(&j.service, "clearMessage", &[peer], false);
        }
    }
    fn device_name(&self) -> String {
        JNI.get()
            .and_then(|j| call(&j.service, "deviceName", &[], true))
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Android phone".into())
    }
    fn device_maker(&self) -> String {
        JNI.get()
            .and_then(|j| call(&j.service, "maker", &[], true))
            .map(|m| m.trim().to_owned())
            .unwrap_or_default()
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_orgtree_hubchat_ConnectionService_00024Companion_startCore(
    env: JNIEnv,
    _this: JObject,
    data_dir: JString,
) {
    start_core(env, data_dir);
}

#[no_mangle]
pub extern "system" fn Java_dev_orgtree_hubchat_ConnectionService_startCore(
    env: JNIEnv,
    _class: JClass,
    data_dir: JString,
) {
    start_core(env, data_dir);
}

/// The periodic check (design D6), called by CheckWorker on its own thread:
/// blocks until the check is done or `timeout_secs` pass. True when every
/// hub answered and nothing waits to be sent.
fn check_now(timeout_secs: i32) -> bool {
    let Ok(c) = crate::core::get() else {
        return false;
    };
    let Ok(e) = c.engine() else {
        return true; // no identity yet: nothing to check
    };
    let t = std::time::Duration::from_secs(timeout_secs.clamp(5, 600) as u64);
    let _check = c.background_check();
    c.rt.block_on(async move { e.check_now(t).await })
}

#[derive(serde::Deserialize)]
struct PushCapability {
    endpoint: String,
    p256dh: String,
    auth: String,
}

/// Runs only on a WorkManager thread; all network work has an overall deadline.
fn push_work(action: &str, capability: &str) -> String {
    let Ok(c) = crate::core::get() else { return "Hubchat is still starting.".into() };
    let Ok(e) = c.engine() else {
        return if action == "remove" { String::new() } else { "Connect an identity before enabling push.".into() };
    };
    let _check = c.background_check();
    c.rt.block_on(async {
        let task = async {
            match action {
                "remove" => e.unregister_push().await.map_err(|e| e.to_string()),
                "sync" => {
                    let cap: PushCapability = serde_json::from_str(capability)
                        .map_err(|_| "Push registration is unavailable.".to_string())?;
                    if !e.check_now(std::time::Duration::from_secs(45)).await {
                        return Err("A hub is unreachable; push setup will retry.".into());
                    }
                    e.register_push(&cap.endpoint, &cap.p256dh, &cap.auth).await.map_err(|e| e.to_string())
                }
                "wake" => if e.check_now(std::time::Duration::from_secs(60)).await {
                    Ok(())
                } else { Err("A hub is unreachable; the message check will retry.".into()) },
                _ => Err("Unknown push operation.".into()),
            }
        };
        match tokio::time::timeout(std::time::Duration::from_secs(100), task).await {
            Ok(Ok(())) => String::new(),
            Ok(Err(e)) => e,
            Err(_) => "The hub did not answer in time; push setup will retry.".into(),
        }
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_orgtree_hubchat_ConnectionService_pushWork(
    env: JNIEnv, _class: JClass, action: JString, capability: JString,
) -> jni::sys::jstring {
    push_work_jni(env, action, capability)
}

#[no_mangle]
pub extern "system" fn Java_dev_orgtree_hubchat_ConnectionService_00024Companion_pushWork(
    env: JNIEnv, _this: JObject, action: JString, capability: JString,
) -> jni::sys::jstring {
    push_work_jni(env, action, capability)
}

fn push_work_jni(
    mut env: JNIEnv, action: JString, capability: JString,
) -> jni::sys::jstring {
    let action = env.get_string(&action).map(String::from).unwrap_or_default();
    let capability = env.get_string(&capability).map(String::from).unwrap_or_default();
    env.new_string(push_work(&action, &capability)).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_dev_orgtree_hubchat_ConnectionService_pushModeChanged(
    _env: JNIEnv, _class: JClass,
) {
    if let Ok(c) = crate::core::get() { c.reconcile_background(); }
}

#[no_mangle]
pub extern "system" fn Java_dev_orgtree_hubchat_ConnectionService_00024Companion_pushModeChanged(
    _env: JNIEnv, _this: JObject,
) {
    if let Ok(c) = crate::core::get() { c.reconcile_background(); }
}

#[no_mangle]
pub extern "system" fn Java_dev_orgtree_hubchat_ConnectionService_00024Companion_checkNow(
    _env: JNIEnv,
    _this: JObject,
    timeout_secs: jni::sys::jint,
) -> jni::sys::jboolean {
    check_now(timeout_secs) as jni::sys::jboolean
}

#[no_mangle]
pub extern "system" fn Java_dev_orgtree_hubchat_ConnectionService_checkNow(
    _env: JNIEnv,
    _class: JClass,
    timeout_secs: jni::sys::jint,
) -> jni::sys::jboolean {
    check_now(timeout_secs) as jni::sys::jboolean
}

fn push_foreground(mut env: JNIEnv, on: JString) {
    let visible = env.get_string(&on).map(String::from).unwrap_or_default() == "1";
    if let Ok(c) = crate::core::get() { c.set_app_visible(visible); }
}

#[no_mangle]
pub extern "system" fn Java_dev_orgtree_hubchat_ConnectionService_pushForeground(
    env: JNIEnv, _class: JClass, on: JString,
) { push_foreground(env, on); }

#[no_mangle]
pub extern "system" fn Java_dev_orgtree_hubchat_ConnectionService_00024Companion_pushForeground(
    env: JNIEnv, _this: JObject, on: JString,
) { push_foreground(env, on); }

fn start_core(mut env: JNIEnv, data_dir: JString) {
    let Ok(dir) = env.get_string(&data_dir).map(String::from) else {
        return;
    };
    if JNI.get().is_none() {
        let (Ok(vm), Ok(service), Ok(secret_box)) = (
            env.get_java_vm(),
            env.find_class("dev/orgtree/hubchat/ConnectionService"),
            env.find_class("dev/orgtree/hubchat/SecretBox"),
        ) else {
            return;
        };
        let (Ok(service), Ok(secret_box)) =
            (env.new_global_ref(service), env.new_global_ref(secret_box))
        else {
            return;
        };
        let _ = JNI.set(Jni {
            vm,
            service,
            secret_box,
        });
    }
    let dir = PathBuf::from(dir);
    let _ = crate::core::init(dir.clone(), Arc::new(AndroidPlatform { dir }));
}
