use std::ffi::{c_int, c_void};
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};
use std::time::Instant;

// The Linux host already links these GTK 4 system libraries. GTK widgets
// are read only in an idle callback on the default GTK main context; the
// engine thread only attaches that source and reads atomics.
#[link(name = "gtk-4")]
unsafe extern "C" {
    fn gtk_window_get_toplevels() -> *mut c_void;
    fn gtk_window_is_active(window: *mut c_void) -> c_int;
    fn gtk_widget_get_mapped(widget: *mut c_void) -> c_int;
}
#[link(name = "gio-2.0")]
unsafe extern "C" {
    fn g_list_model_get_n_items(model: *mut c_void) -> u32;
    fn g_list_model_get_item(model: *mut c_void, position: u32) -> *mut c_void;
}
#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_object_unref(object: *mut c_void);
}
#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_idle_add_full(
        priority: c_int,
        function: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
        data: *mut c_void,
        notify: Option<unsafe extern "C" fn(*mut c_void)>,
    ) -> u32;
}

// 0 = unknown, 1 = no active mapped window, 2 = active mapped window.
static STATE: AtomicU8 = AtomicU8::new(0);
static PENDING: AtomicBool = AtomicBool::new(false);
static UPDATED_MS: AtomicU64 = AtomicU64::new(0);
static START: OnceLock<Instant> = OnceLock::new();

fn clock_ms() -> u64 {
    // Offset leaves zero available for "the GTK callback has never run".
    START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

unsafe extern "C" fn sample_on_gtk_thread(_: *mut c_void) -> c_int {
    let mut state = 0;
    // gtk_window_get_toplevels() is transfer-none. Each item returned by
    // GListModel is transfer-full and must be unreferenced once.
    let windows = unsafe { gtk_window_get_toplevels() };
    if !windows.is_null() {
        let count = unsafe { g_list_model_get_n_items(windows) };
        if count <= 10_000 {
            state = 1;
            for index in 0..count {
                let window = unsafe { g_list_model_get_item(windows, index) };
                if window.is_null() {
                    state = 0;
                    continue;
                }
                let active = unsafe {
                    gtk_window_is_active(window) != 0 && gtk_widget_get_mapped(window) != 0
                };
                unsafe { g_object_unref(window) };
                if active {
                    state = 2;
                    break;
                }
            }
        }
    }
    STATE.store(state, Ordering::Relaxed);
    UPDATED_MS.store(clock_ms(), Ordering::Release);
    PENDING.store(false, Ordering::Release);
    0 // G_SOURCE_REMOVE: every poll schedules at most one fresh sample.
}

pub(super) fn foreground() -> Option<bool> {
    if !PENDING.swap(true, Ordering::AcqRel) {
        // g_idle_add_full is thread-safe and always attaches to the global
        // default context. Unlike MainContext::invoke it cannot call GTK
        // inline on the engine thread before the main loop has acquired it.
        let source =
            unsafe { g_idle_add_full(200, Some(sample_on_gtk_thread), std::ptr::null_mut(), None) };
        if source == 0 {
            PENDING.store(false, Ordering::Release);
            return None;
        }
    }
    let updated = UPDATED_MS.load(Ordering::Acquire);
    if updated == 0 || clock_ms().saturating_sub(updated) > 2_000 {
        // A stopped/stalled GTK loop must not leave an old true value alive.
        return None;
    }
    match STATE.load(Ordering::Relaxed) {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}
