//! System application activity, safe to read on NSH's JavaScript engine thread.
//!
//! AppKit and GTK widgets belong to the platform thread. macOS uses OS read
//! queries; Linux schedules GTK reads on its main loop. iOS observes UIKit on
//! the main thread before the runner starts. A missing GUI session or
//! inconclusive query is unknown, rather than an invented lifecycle event.

pub fn foreground(os: &str) -> Option<bool> {
    #[cfg(target_os = "ios")]
    if os == "ios" {
        return platform::foreground();
    }
    #[cfg(target_os = "android")]
    if os == "android" {
        return super::bootstrap::foreground();
    }
    #[cfg(target_os = "macos")]
    if os == "macos" {
        return platform::foreground();
    }
    #[cfg(target_os = "windows")]
    if os == "windows" {
        return platform::foreground();
    }
    #[cfg(target_os = "linux")]
    if os == "linux" {
        return platform::foreground();
    }
    let _ = os;
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
#[path = "ios.rs"]
mod platform;

#[cfg(target_os = "ios")]
pub(super) use platform::{install, observe};

#[cfg(target_os = "ios")]
pub(super) use ios_observers::ActivityObserver;

#[cfg(any(target_os = "ios", test))]
mod ios_observers;

#[cfg(any(target_os = "ios", test))]
mod ios_state;
