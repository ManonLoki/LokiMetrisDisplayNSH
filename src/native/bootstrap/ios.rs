//! Standalone NSH startup using the existing iOS container and private Keychain.
use crate::native::{connection, identity, lifecycle};
use serde_json::{Value, json};
use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    ptr,
    sync::Weak,
};

const DEFAULT_NAME: &str = "LokiMetris Display";

/// Restore the same iOS identity and files without a Swift host.
pub(super) fn configure(
    _root: &Path,
    _device_id: &str,
    _name: &str,
) -> Result<connection::Config, String> {
    let bundle = bundle_id()?;
    let device_id = identity::device_identity(&format!("{bundle}.lan-identity-v1"))
        .map_err(|failure| failure.message().to_owned())?;
    // The former FileManager applicationSupportDirectory and the NSH iOS
    // runtime both resolve to this sandbox directory. Keep the old owner root.
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or("应用私有目录不可用")?;
    let root = home.join("Library/Application Support");
    let saved = preference("mdns.deviceName").and_then(|value| value.as_str().map(str::to_owned));
    let name = saved
        .filter(|value| crate::shared::domain::identity::apple_name_error(value).is_none())
        .unwrap_or_else(|| DEFAULT_NAME.into());
    let legacy_panel = legacy_panel(&root);
    serde_json::from_value(json!({
        "root": root, "deviceId": device_id, "name": name,
        "hostIdentity": true, "hostLifecycle": true, "platform": "ios",
        "appVersion": bundle_string("CFBundleShortVersionString").unwrap_or_else(|| "1.0".into()),
        "legacyPanel": legacy_panel,
    }))
    .map_err(|error| format!("iOS连接配置无效: {error}"))
}

/// The observer owns only a weak reference; closing the VM closes its connection.
pub(super) fn observe(handle: Weak<connection::Handle>) -> lifecycle::ActivityObserver {
    lifecycle::observe(move |visible| {
        let Some(handle) = handle.upgrade() else {
            return;
        };
        let command = json!({"op":"lifecycle","visible":visible,"permitted":true});
        let _ = handle.host_command(&command.to_string());
    })
}

fn legacy_panel(root: &Path) -> Option<Value> {
    let persisted = std::fs::read(root.join("panel-v1.json"))
        .ok()
        .filter(|bytes| bytes.len() <= 64 * 1024)
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(Value::is_object);
    persisted.or_else(|| {
        let mut values = serde_json::Map::new();
        for (field, key) in [
            ("showUsage", "panel.showUsage"),
            ("showHooks", "panel.showHooks"),
            ("usageDetail", "panel.usageDetail"),
            ("hookLayout", "panel.hookLayout"),
        ] {
            if let Some(value) = preference(key) {
                values.insert(field.into(), value);
            }
        }
        (!values.is_empty()).then_some(Value::Object(values))
    })
}

type Cf = *const c_void;
const UTF8: u32 = 0x0800_0100;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFBundleGetMainBundle() -> Cf;
    fn CFBundleGetIdentifier(bundle: Cf) -> Cf;
    fn CFBundleGetValueForInfoDictionaryKey(bundle: Cf, key: Cf) -> Cf;
    fn CFStringCreateWithBytes(
        allocator: Cf,
        bytes: *const u8,
        length: isize,
        encoding: u32,
        external: u8,
    ) -> Cf;
    fn CFStringGetCString(value: Cf, buffer: *mut u8, capacity: isize, encoding: u32) -> u8;
    fn CFPreferencesCopyAppValue(key: Cf, application: Cf) -> Cf;
    fn CFGetTypeID(value: Cf) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(value: Cf) -> u8;
    fn CFNumberGetTypeID() -> usize;
    fn CFNumberGetValue(value: Cf, kind: isize, result: *mut c_void) -> u8;
    fn CFRelease(value: Cf);
    static kCFPreferencesCurrentApplication: Cf;
}

struct Owned(Cf);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this wrapper owns one Create/Copy reference.
            unsafe { CFRelease(self.0) };
        }
    }
}

fn key(text: &str) -> Owned {
    // SAFETY: text is a valid UTF-8 buffer for the duration of the call.
    Owned(unsafe {
        CFStringCreateWithBytes(ptr::null(), text.as_ptr(), text.len() as isize, UTF8, 0)
    })
}

fn string(value: Cf) -> Option<String> {
    // SAFETY: callers pass a live CoreFoundation value or NULL.
    if value.is_null() || unsafe { CFGetTypeID(value) != CFStringGetTypeID() } {
        return None;
    }
    let mut bytes = [0_u8; 4096];
    // SAFETY: type was checked and the writable buffer has the advertised capacity.
    if unsafe { CFStringGetCString(value, bytes.as_mut_ptr(), bytes.len() as isize, UTF8) } == 0 {
        return None;
    }
    std::ffi::CStr::from_bytes_until_nul(&bytes)
        .ok()?
        .to_str()
        .ok()
        .map(str::to_owned)
}

fn bundle_id() -> Result<String, String> {
    // SAFETY: main bundle is a process-lifetime borrowed CoreFoundation object.
    let bundle = unsafe { CFBundleGetMainBundle() };
    if bundle.is_null() {
        return Err("应用标识不可用".into());
    }
    // SAFETY: bundle is live and CFBundleGetIdentifier returns a borrowed string.
    string(unsafe { CFBundleGetIdentifier(bundle) })
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "应用标识不可用".into())
}

fn bundle_string(name: &str) -> Option<String> {
    let key = key(name);
    // SAFETY: main bundle is process-owned and borrowed; key owns its CFString.
    let bundle = unsafe { CFBundleGetMainBundle() };
    if bundle.is_null() || key.0.is_null() {
        return None;
    }
    // SAFETY: both CFBundle and key are live for this synchronous lookup.
    string(unsafe { CFBundleGetValueForInfoDictionaryKey(bundle, key.0) })
}

fn preference(name: &str) -> Option<Value> {
    let key = key(name);
    if key.0.is_null() {
        return None;
    }
    // SAFETY: key is live; current-application is an immutable CF constant.
    let value =
        Owned(unsafe { CFPreferencesCopyAppValue(key.0, kCFPreferencesCurrentApplication) });
    if value.0.is_null() {
        return None;
    }
    if let Some(text) = string(value.0) {
        return Some(Value::String(text));
    }
    // SAFETY: copied value remains owned for the complete type/value inspection.
    let kind = unsafe { CFGetTypeID(value.0) };
    // SAFETY: CF type identifier functions read immutable runtime metadata.
    if kind == unsafe { CFBooleanGetTypeID() } {
        // SAFETY: value's CFBoolean type was checked.
        return Some(Value::Bool(unsafe { CFBooleanGetValue(value.0) } != 0));
    }
    // SAFETY: CF type identifier functions read immutable runtime metadata.
    if kind == unsafe { CFNumberGetTypeID() } {
        let mut number = 0_i64;
        // SAFETY: CFNumber type checked, kCFNumberSInt64Type=4, output is an i64.
        if unsafe { CFNumberGetValue(value.0, 4, (&mut number as *mut i64).cast()) } != 0 {
            return Some(json!(number));
        }
    }
    None
}
