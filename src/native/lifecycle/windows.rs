use std::ffi::c_void;
#[link(name = "user32")]
unsafe extern "system" {
    fn GetForegroundWindow() -> *mut c_void;
    fn GetWindowThreadProcessId(window: *mut c_void, pid: *mut u32) -> u32;
    fn IsWindowVisible(window: *mut c_void) -> i32;
    fn IsIconic(window: *mut c_void) -> i32;
}

pub(super) fn foreground() -> Option<bool> {
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        // Windows documents a transient NULL while activation is changing.
        return None;
    }
    let mut pid = 0;
    if unsafe { GetWindowThreadProcessId(window, &mut pid) } == 0 || pid == 0 {
        return None;
    }
    if pid != std::process::id() {
        return Some(false);
    }
    Some(unsafe { IsWindowVisible(window) != 0 && IsIconic(window) == 0 })
}
