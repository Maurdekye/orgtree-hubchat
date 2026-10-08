//! JNI bridge for ConnectionService.kt. The service calls `startCore` when it
//! is created (also after a START_STICKY restart with no activity), and the
//! core calls back into the service's static methods to post notifications.

use std::path::PathBuf;
use std::sync::Arc;

use jni::objects::{GlobalRef, JClass, JString, JValue};
use jni::{JNIEnv, JavaVM};

use crate::connection::{self, Platform};

struct AndroidPlatform {
    vm: JavaVM,
    // The class must be resolved on a Java thread: FindClass from the core's
    // own threads only sees the system class loader.
    class: GlobalRef,
}

impl AndroidPlatform {
    fn call(&self, method: &str, args: &[&str]) {
        let Ok(mut env) = self.vm.attach_current_thread_as_daemon() else {
            return;
        };
        let strings: Vec<_> = args.iter().filter_map(|a| env.new_string(a).ok()).collect();
        if strings.len() != args.len() {
            return;
        }
        let values: Vec<JValue> = strings.iter().map(|s| JValue::Object(s.as_ref())).collect();
        let sig = format!("({})V", "Ljava/lang/String;".repeat(args.len()));
        let class: &JClass = self.class.as_obj().into();
        if env
            .call_static_method(class, method, &sig, &values)
            .is_err()
        {
            let _ = env.exception_clear();
        }
    }
}

impl Platform for AndroidPlatform {
    fn notify(&self, title: &str, body: &str) {
        self.call("notifyMessage", &[title, body]);
    }
    fn status(&self, text: &str) {
        self.call("setStatus", &[text]);
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_orgtree_hubchat_ConnectionService_00024Companion_startCore(
    env: JNIEnv,
    _this: jni::objects::JObject,
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
    let Ok(vm) = env.get_java_vm() else { return };
    let Ok(class) = env.find_class("dev/orgtree/hubchat/ConnectionService") else {
        return;
    };
    let Ok(class) = env.new_global_ref(class) else {
        return;
    };
    connection::start(PathBuf::from(dir), Arc::new(AndroidPlatform { vm, class }));
}
