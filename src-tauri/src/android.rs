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
    fn notify(&self, title: &str, body: &str, peer: &str) {
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
    fn download_dir(&self) -> PathBuf {
        self.dir.join("downloads")
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
