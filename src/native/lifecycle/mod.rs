//! System application activity, safe to read on NSH's JavaScript engine thread.
//!
//! AppKit and GTK widgets belong to the platform thread. macOS uses OS read
//! queries; Linux schedules GTK reads on its main loop. iOS observes UIKit on
//! the main thread before the runner starts. A missing GUI session or
//! inconclusive query is unknown, rather than an invented lifecycle event.

/// `os` is the platform NSH reports; a simulated platform that differs from
/// the compiled target has no system facts and stays unknown.
pub fn foreground(os: &str) -> Option<bool> {
    if os != std::env::consts::OS {
        return None;
    }
    #[cfg(target_os = "android")]
    return super::bootstrap::foreground();
    #[cfg(any(
        target_os = "ios",
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    ))]
    return platform::foreground();
    #[cfg(not(any(
        target_os = "android",
        target_os = "ios",
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    )))]
    None
}

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;

#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;

#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod lifecycle_test;

#[cfg(target_os = "ios")]
#[path = "ios/mod.rs"]
mod platform;

#[cfg(target_os = "ios")]
pub(super) use platform::{install, observe};

#[cfg(target_os = "ios")]
pub(super) use ios_observers::ActivityObserver;

#[cfg(any(target_os = "ios", test))]
#[path = "ios/observers.rs"]
mod ios_observers;

#[cfg(any(target_os = "ios", test))]
#[path = "ios/state.rs"]
mod ios_state;
