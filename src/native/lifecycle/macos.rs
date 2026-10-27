use std::ffi::c_void;

type CfRef = *const c_void;
#[repr(C)]
struct ProcessSerialNumber {
    high: u32,
    low: u32,
}

// Process Manager and WindowServer queries do not access NSApplication or
// NSWindow. The deprecated Process Manager API remains an exported OS API;
// using it here avoids dispatching a synchronous UI query onto the main loop.
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn GetFrontProcess(process: *mut ProcessSerialNumber) -> i16;
    fn GetProcessPID(process: *const ProcessSerialNumber, pid: *mut i32) -> i32;
}
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(options: u32, relative_to: u32) -> CfRef;
    static kCGWindowOwnerPID: CfRef;
    static kCGWindowLayer: CfRef;
    static kCGWindowAlpha: CfRef;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: CfRef);
    fn CFGetTypeID(value: CfRef) -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFArrayGetCount(array: CfRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CfRef, index: isize) -> CfRef;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFDictionaryGetValue(dictionary: CfRef, key: CfRef) -> CfRef;
    fn CFNumberGetTypeID() -> usize;
    fn CFNumberGetValue(number: CfRef, kind: isize, result: *mut c_void) -> u8;
}

struct OwnedCf(CfRef);
impl Drop for OwnedCf {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

unsafe fn number_i32(dictionary: CfRef, key: CfRef) -> Option<i32> {
    let value = unsafe { CFDictionaryGetValue(dictionary, key) };
    if value.is_null() || unsafe { CFGetTypeID(value) != CFNumberGetTypeID() } {
        return None;
    }
    let mut result = 0_i32;
    // kCFNumberSInt32Type = 3.
    (unsafe { CFNumberGetValue(value, 3, (&mut result as *mut i32).cast()) } != 0).then_some(result)
}

pub(super) fn foreground() -> Option<bool> {
    let mut process = ProcessSerialNumber { high: 0, low: 0 };
    let mut pid = 0_i32;
    if unsafe { GetFrontProcess(&mut process) } != 0
        || unsafe { GetProcessPID(&process, &mut pid) } != 0
        || pid <= 0
    {
        return None;
    }
    if pid as u32 != std::process::id() {
        return Some(false);
    }
    // OnscreenOnly | ExcludeDesktopElements. A frontmost application with
    // all its normal windows minimized/hidden is still inactive for effects.
    let list = unsafe { CGWindowListCopyWindowInfo((1 << 0) | (1 << 4), 0) };
    if list.is_null() {
        return None;
    }
    let list = OwnedCf(list);
    if unsafe { CFGetTypeID(list.0) != CFArrayGetTypeID() } {
        return None;
    }
    for index in 0..unsafe { CFArrayGetCount(list.0) } {
        let window = unsafe { CFArrayGetValueAtIndex(list.0, index) };
        if window.is_null()
            || unsafe { CFGetTypeID(window) != CFDictionaryGetTypeID() }
            || unsafe { number_i32(window, kCGWindowOwnerPID) } != Some(pid)
            || unsafe { number_i32(window, kCGWindowLayer) } != Some(0)
        {
            continue;
        }
        let alpha = unsafe { CFDictionaryGetValue(window, kCGWindowAlpha) };
        if alpha.is_null() || unsafe { CFGetTypeID(alpha) != CFNumberGetTypeID() } {
            return None;
        }
        let mut opacity = 0_f64;
        // kCFNumberFloat64Type = 6.
        if unsafe { CFNumberGetValue(alpha, 6, (&mut opacity as *mut f64).cast()) } == 0 {
            return None;
        }
        if opacity.is_finite() && opacity > 0.0 {
            return Some(true);
        }
    }
    Some(false)
}
