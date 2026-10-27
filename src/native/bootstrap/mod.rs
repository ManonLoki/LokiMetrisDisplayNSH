//! Minimal system bootstrap for the NSH-owned runtime; no product host or system extension.
use super::connection::Config;
use std::path::Path;

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "ios")]
mod ios;

pub(super) fn configure(root: &Path, device_id: &str, name: &str) -> Result<Config, String> {
    #[cfg(target_os = "android")]
    return android::configure(root, device_id, name);
    #[cfg(target_os = "ios")]
    return ios::configure(root, device_id, name);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    serde_json::from_value(serde_json::json!({"root":root,"deviceId":device_id,"name":name}))
        .map_err(|error| error.to_string())
}

pub(super) fn form_factor() -> String {
    #[cfg(target_os = "android")]
    return android::device_form();
    #[cfg(not(target_os = "android"))]
    "desktop".into()
}

/// Keeps system activity observation alive for as long as the Link owns it.
#[cfg(target_os = "ios")]
pub(super) type ActivityGuard = super::lifecycle::ActivityObserver;
#[cfg(not(target_os = "ios"))]
pub(super) type ActivityGuard = ();

/// Starts pushing system foreground facts into the owner; desktop polls them instead.
pub(super) fn observe(handle: std::sync::Weak<super::connection::Handle>) -> ActivityGuard {
    #[cfg(target_os = "ios")]
    return ios::observe(handle);
    #[cfg(target_os = "android")]
    return android::observe(handle);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    drop(handle)
}

#[cfg(target_os = "android")]
pub(super) fn foreground() -> Option<bool> {
    android::foreground()
}

#[cfg(any(target_os = "android", test))]
#[path = "android/key_record.rs"]
mod android_key_record;
