//! Read-only system activity and input form facts for the NSH UI.
use super::{bootstrap, lifecycle};
use nsh_core::prelude::*;

/// Read-only NSH application activity and input form facts.
#[module]
pub struct UiLifecycle;

#[module]
impl UiLifecycle {
    /// Returns unknown when the current platform cannot establish foreground visibility.
    pub fn foreground(sys: System) -> Option<bool> {
        lifecycle::foreground(sys.os.name())
    }

    /// Identifies television input without exposing retired system-extension capabilities.
    pub fn form_factor() -> String {
        bootstrap::form_factor()
    }
}
