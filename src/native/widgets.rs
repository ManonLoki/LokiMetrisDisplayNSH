//! WidgetKit system access; the authenticated owner alone publishes private read targets.
#[cfg(target_os = "ios")]
use std::{
    ffi::{CStr, c_char},
    path::PathBuf,
};

#[cfg(target_os = "ios")]
unsafe extern "C" {
    fn loki_widget_host_directory() -> *mut c_char;
    fn loki_widget_host_free(value: *mut c_char);
    fn loki_widget_host_reload();
    fn loki_widget_host_bootstrap();
}

/// Installs the process's system refresh entry points, without creating another LAN owner.
pub(crate) fn install() {
    #[cfg(target_os = "ios")]
    {
        // The final Swift host object references these C exports. Preserve their
        // addresses through Rust LTO without executing either entry point.
        let call: unsafe extern "C" fn(*const c_char) -> *mut c_char =
            loki_widget_core::ffi::loki_core_call;
        let free: unsafe extern "C" fn(*mut c_char) = loki_widget_core::ffi::loki_core_free;
        std::hint::black_box((call, free));
        // SAFETY: the statically linked Swift shim owns its idempotent system registration.
        unsafe { loki_widget_host_bootstrap() };
    }
}

/// Accepts only the system-granted App Group directory returned by the native shim.
#[cfg(target_os = "ios")]
pub(super) fn directory() -> Option<PathBuf> {
    // SAFETY: the shim returns an owned NUL-terminated allocation, or null.
    let raw = unsafe { loki_widget_host_directory() };
    if raw.is_null() {
        return None;
    }
    // SAFETY: raw remains live until the matching shim free below.
    let path = unsafe { CStr::from_ptr(raw) }
        .to_str()
        .ok()
        .map(PathBuf::from);
    // SAFETY: this is the shim's allocation and is freed exactly once.
    unsafe { loki_widget_host_free(raw) };
    path.filter(|path| path.is_absolute() && path.is_dir())
}

#[cfg(any(target_os = "ios", test))]
#[derive(Default)]
struct ReloadGate {
    last: Option<std::time::Instant>,
    pending: bool,
}

#[cfg(any(target_os = "ios", test))]
impl ReloadGate {
    fn request(&mut self, now: std::time::Instant) -> Option<std::time::Duration> {
        if self.pending {
            return None;
        }
        let interval = std::time::Duration::from_secs(30);
        let remaining = self
            .last
            .and_then(|last| interval.checked_sub(now.saturating_duration_since(last)))
            .unwrap_or_default();
        if remaining.is_zero() {
            self.last = Some(now);
        } else {
            self.pending = true;
        }
        Some(remaining)
    }
}

/// Coalesces changed-summary refreshes, retaining one trailing update per thirty seconds.
pub(super) fn changed() {
    #[cfg(target_os = "ios")]
    {
        use std::{
            sync::{Mutex, OnceLock},
            time::Instant,
        };
        static GATE: OnceLock<Mutex<ReloadGate>> = OnceLock::new();
        let gate = GATE.get_or_init(|| Mutex::new(ReloadGate::default()));
        let delay = gate
            .lock()
            .expect("widget reload gate poisoned")
            .request(Instant::now());
        let Some(delay) = delay else {
            return;
        };
        if delay.is_zero() {
            // SAFETY: the native shim dispatches WidgetKit access to the system queue.
            unsafe { loki_widget_host_reload() };
        } else if let Ok(when) = dispatch2::DispatchTime::try_from(delay) {
            let _ = dispatch2::DispatchQueue::main().after(when, move || {
                let mut state = gate.lock().expect("widget reload gate poisoned");
                state.pending = false;
                state.last = Some(Instant::now());
                drop(state);
                // SAFETY: callback runs on the main queue and the shim is process-lived.
                unsafe { loki_widget_host_reload() };
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    #[test]
    fn reloads_coalesce_and_preserve_the_trailing_change() {
        let now = Instant::now();
        let mut gate = ReloadGate::default();
        assert_eq!(gate.request(now), Some(Duration::ZERO));
        assert_eq!(
            gate.request(now + Duration::from_secs(5)),
            Some(Duration::from_secs(25))
        );
        assert_eq!(gate.request(now + Duration::from_secs(6)), None);
        gate.pending = false;
        gate.last = Some(now + Duration::from_secs(30));
        assert_eq!(
            gate.request(now + Duration::from_secs(60)),
            Some(Duration::ZERO)
        );
    }
}
