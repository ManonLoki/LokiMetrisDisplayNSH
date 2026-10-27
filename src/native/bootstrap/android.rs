//! Bootstrap a standalone NSH app using the existing Android identity and encrypted vault.
#[path = "android_context.rs"]
mod context;
#[path = "android_jni.rs"]
mod jni_support;
#[path = "android_keys.rs"]
mod keys;

use crate::shared::domain::identity::{android_default_name, android_name_error, valid_id};
use jni::{
    JNIEnv,
    objects::{JObject, JValue},
};
use jni_support::{Failure, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

fn files_dir(env: &mut JNIEnv<'_>, context: &JObject<'_>) -> Result<PathBuf> {
    let directory = env
        .call_method(context, "getFilesDir", "()Ljava/io/File;", &[])?
        .l()?;
    let path = env
        .call_method(directory, "getAbsolutePath", "()Ljava/lang/String;", &[])?
        .l()?;
    Ok(jni_support::text(env, path)?.into())
}

fn identity(env: &mut JNIEnv<'_>, context: &JObject<'_>) -> Result<(String, String)> {
    let prefs = jni_support::preferences(env, context, "mdns_identity")?;
    let stored_id = jni_support::pref_string(env, &prefs, "device_id")?;
    let id = match stored_id {
        Some(id) if valid_id(&id) => id,
        Some(_) => return Err(jni_support::invalid("既有设备身份无效，已保留原记录。")),
        None => uuid::Uuid::new_v4().to_string(),
    };
    let stored_name = jni_support::pref_string(env, &prefs, "device_name")?;
    let name = match stored_name.filter(|name| android_name_error(name).is_none()) {
        Some(name) => name,
        None => {
            let model = env
                .get_static_field("android/os/Build", "MODEL", "Ljava/lang/String;")?
                .l()?;
            android_default_name(&jni_support::text(env, model)?)
        }
    };
    jni_support::save_strings(env, &prefs, &[("device_id", &id), ("device_name", &name)])?;
    Ok((id, name))
}

fn read_device_form(env: &mut JNIEnv<'_>, context: &JObject<'_>) -> Result<&'static str> {
    let manager = jni_support::system_service(env, context, "uimode")?;
    let mode = env
        .call_method(manager, "getCurrentModeType", "()I", &[])?
        .i()?;
    if mode == 4 {
        return Ok("tv");
    }
    let resources = env
        .call_method(
            context,
            "getResources",
            "()Landroid/content/res/Resources;",
            &[],
        )?
        .l()?;
    let config = env
        .call_method(
            resources,
            "getConfiguration",
            "()Landroid/content/res/Configuration;",
            &[],
        )?
        .l()?;
    let width = env.get_field(config, "smallestScreenWidthDp", "I")?.i()?;
    Ok(if width >= 600 { "tablet" } else { "phone" })
}

fn legacy_panel(env: &mut JNIEnv<'_>, context: &JObject<'_>) -> Result<serde_json::Value> {
    let prefs = jni_support::preferences(env, context, "mdns_identity")?;
    let saved = env
        .call_method(prefs, "getAll", "()Ljava/util/Map;", &[])?
        .l()?;
    let mut panel = serde_json::Map::new();
    for (key, stored) in [
        ("showUsage", "panel.show_usage"),
        ("showHooks", "panel.show_hooks"),
        ("usageDetail", "panel.usage_detail"),
        ("hookLayout", "panel.hook_layout"),
    ] {
        let stored = jni_support::string(env, stored)?;
        let value = env
            .call_method(
                &saved,
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                &[JValue::Object(&stored)],
            )?
            .l()?;
        if value.is_null() {
            continue;
        }
        if env.is_instance_of(&value, "java/lang/Boolean")? {
            panel.insert(
                key.into(),
                json!(env.call_method(value, "booleanValue", "()Z", &[])?.z()?),
            );
        } else if env.is_instance_of(&value, "java/lang/String")? {
            panel.insert(key.into(), json!(jni_support::text(env, value)?));
        }
    }
    Ok(if panel.is_empty() {
        serde_json::Value::Null
    } else {
        panel.into()
    })
}

/// Uses the old application directory and keeps secrets entirely inside native code.
pub(super) fn configure(
    _root: &Path,
    _device_id: &str,
    _name: &str,
) -> std::result::Result<crate::native::connection::Config, String> {
    context::with_context(|env, context| -> Result<_> {
        let root = files_dir(env, context)?;
        let (device_id, name) = identity(env, context)?;
        let mut key = keys::load(env, context, &root.join("loki-link-v1/vault"))?;
        let result =
            crate::native::credentials::configure_vault(root.join("loki-link-v1/vault"), key);
        key.fill(0);
        result.map_err(Failure)?;
        let form = read_device_form(env, context)?;
        serde_json::from_value(json!({
            "root":root, "deviceId":device_id, "name":name, "hostIdentity":true,
            "hostLifecycle":true, "platform":"android", "keepSnapshot":false,
            "formFactor":(form != "tv").then_some(form),
            "legacyPanel":legacy_panel(env, context)?,
        }))
        .map_err(|_| jni_support::invalid("Android 连接配置无效。"))
    })
    .map_err(|error: Failure| error.0)
}

/// Queries the OS process importance without claiming that a background app is visible.
pub(super) fn foreground() -> Option<bool> {
    context::with_context(|env, context| -> Result<_> {
        let info = env.new_object(
            "android/app/ActivityManager$RunningAppProcessInfo",
            "()V",
            &[],
        )?;
        env.call_static_method(
            "android/app/ActivityManager",
            "getMyMemoryState",
            "(Landroid/app/ActivityManager$RunningAppProcessInfo;)V",
            &[JValue::Object(&info)],
        )?;
        let importance = env.get_field(info, "importance", "I")?.i()?;
        let power = jni_support::system_service(env, context, "power")?;
        let interactive = env.call_method(power, "isInteractive", "()Z", &[])?.z()?;
        let keyguard = jni_support::system_service(env, context, "keyguard")?;
        let locked = env
            .call_method(keyguard, "isKeyguardLocked", "()Z", &[])?
            .z()?;
        // IMPORTANCE_VISIBLE also covers an unfocused split-screen Activity and permission UI.
        Ok(importance <= 200 && interactive && !locked)
    })
    .ok()
}

/// Reports the system form independently from the connection protocol's phone/tablet field.
pub(super) fn device_form() -> String {
    context::with_context(|env, context| read_device_form(env, context).map(str::to_owned))
        .unwrap_or_else(|_| "phone".into())
}

fn multicast_lock() -> Result<Option<jni::objects::GlobalRef>> {
    context::with_context(|env, context| -> Result<_> {
        let wifi = jni_support::system_service(env, context, "wifi")?;
        if wifi.is_null() {
            return Ok(None);
        }
        let tag = jni_support::string(env, "loki-lan-discovery")?;
        let lock = env
            .call_method(
                wifi,
                "createMulticastLock",
                "(Ljava/lang/String;)Landroid/net/wifi/WifiManager$MulticastLock;",
                &[JValue::Object(&tag)],
            )?
            .l()?;
        env.call_method(&lock, "setReferenceCounted", "(Z)V", &[JValue::Bool(0)])?;
        let lock = env.new_global_ref(lock)?;
        env.call_method(lock.as_obj(), "acquire", "()V", &[])?;
        Ok(Some(lock))
    })
}

fn release_multicast(lock: &jni::objects::GlobalRef) {
    let _: Result<()> = context::with_context(|env, _| {
        env.call_method(lock.as_obj(), "release", "()V", &[])?;
        Ok(())
    });
}

/// A weak observer cannot keep the connection alive after its NSH VM has closed.
pub(super) fn observe(link: std::sync::Weak<crate::native::connection::Handle>) {
    let _ = std::thread::Builder::new()
        .name("loki-android-lifecycle".into())
        .spawn(move || {
            let mut previous = None;
            let mut multicast = None;
            loop {
                {
                    let Some(link) = link.upgrade() else {
                        break;
                    };
                    let visible = foreground().unwrap_or(false);
                    if visible && multicast.is_none() {
                        multicast = multicast_lock().ok().flatten();
                    }
                    if !visible && let Some(lock) = multicast.take() {
                        release_multicast(&lock);
                    }
                    if previous != Some(visible) {
                        let event = json!({"op":"lifecycle", "visible":visible, "permitted":true});
                        if link.host_command(&event.to_string()).is_ok() {
                            previous = Some(visible);
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            if let Some(lock) = multicast {
                release_multicast(&lock);
            }
        });
}
