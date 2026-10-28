// 引入不透明指针类型
use std::ffi::c_void;

// Core Foundation 对象引用
type CfRef = *const c_void;
#[repr(C)]
// 进程序列号
struct ProcessSerialNumber {
    // 高位
    high: u32,
    // 低位
    low: u32,
}

// Process Manager 与 WindowServer 查询不访问 NSApplication 或
// NSWindow。已弃用的 Process Manager API 仍是导出的系统 API;
// 在此使用它可避免向主循环派发同步的 UI 查询。
// Process Manager 与 WindowServer 查询不访问 NSApplication/NSWindow，避免同步派发到主循环
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    // 取最前进程
    fn GetFrontProcess(process: *mut ProcessSerialNumber) -> i16;
    // 序列号转进程 ID
    fn GetProcessPID(process: *const ProcessSerialNumber, pid: *mut i32) -> i32;
}
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    // 取窗口列表
    fn CGWindowListCopyWindowInfo(options: u32, relative_to: u32) -> CfRef;
    // 窗口属性键：所属进程、层级、透明度
    static kCGWindowOwnerPID: CfRef;
    // 窗口层级键
    static kCGWindowLayer: CfRef;
    // 窗口透明度键
    static kCGWindowAlpha: CfRef;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    // 释放对象
    fn CFRelease(value: CfRef);
    // 取类型 ID
    fn CFGetTypeID(value: CfRef) -> usize;
    // 数组类型 ID
    fn CFArrayGetTypeID() -> usize;
    // 数组长度
    fn CFArrayGetCount(array: CfRef) -> isize;
    // 取元素
    fn CFArrayGetValueAtIndex(array: CfRef, index: isize) -> CfRef;
    // 字典类型 ID
    fn CFDictionaryGetTypeID() -> usize;
    // 取字典值
    fn CFDictionaryGetValue(dictionary: CfRef, key: CfRef) -> CfRef;
    // 数字类型 ID
    fn CFNumberGetTypeID() -> usize;
    // 读出数字
    fn CFNumberGetValue(number: CfRef, kind: isize, result: *mut c_void) -> u8;
}

// 持有 CF 对象，离开作用域时释放
struct OwnedCf(CfRef);
// 持有对象的析构
impl Drop for OwnedCf {
    // 销毁时
    fn drop(&mut self) {
        // 释放对象
        unsafe { CFRelease(self.0) };
    }
}

// 从字典读取 32 位整数，类型不符返回 None
unsafe fn number_i32(dictionary: CfRef, key: CfRef) -> Option<i32> {
    // 在字典里按键取值
    let value = unsafe { CFDictionaryGetValue(dictionary, key) };
    // 为空或不是数字类型
    if value.is_null() || unsafe { CFGetTypeID(value) != CFNumberGetTypeID() } {
        // 返回 None
        return None;
    }
    // 数值缓冲
    let mut result = 0_i32;
    // kCFNumberSInt32Type = 3.
    // kCFNumberSInt32Type = 3
    // 读取数值，成功则返回
    (unsafe { CFNumberGetValue(value, 3, (&mut result as *mut i32).cast()) } != 0).then_some(result)
}

// 前台判断：本进程是最前进程且有可见的普通窗口
pub(super) fn foreground() -> Option<bool> {
    // 当前进程的序列号
    let mut process = ProcessSerialNumber { high: 0, low: 0 };
    // 进程号
    let mut pid = 0_i32;
    // 取最前进程失败则未知
    if unsafe { GetFrontProcess(&mut process) } != 0
        // 或者取进程号失败
        || unsafe { GetProcessPID(&process, &mut pid) } != 0
        // 或者进程号非正
        || pid <= 0
    {
        // 返回 None
        return None;
    }
    // 最前进程不是自己
    if pid as u32 != std::process::id() {
        // 进程是前台应用
        return Some(false);
    }
    // OnscreenOnly | ExcludeDesktopElements。即使最前台应用的
    // 所有普通窗口都已最小化/隐藏,对于特效而言它仍然是非活动的。
    // 只取屏幕上的窗口并排除桌面元素；所有普通窗口都被最小化或隐藏时仍视为不活跃
    let list = unsafe { CGWindowListCopyWindowInfo((1 << 0) | (1 << 4), 0) };
    // 窗口列表为空
    if list.is_null() {
        // 返回 None
        return None;
    }
    // 接管列表所有权
    let list = OwnedCf(list);
    // 类型不对则未知
    if unsafe { CFGetTypeID(list.0) != CFArrayGetTypeID() } {
        // 返回 None
        return None;
    }
    // 逐个窗口检查
    for index in 0..unsafe { CFArrayGetCount(list.0) } {
        // 取窗口
        let window = unsafe { CFArrayGetValueAtIndex(list.0, index) };
        // 跳过不属于本进程或不在普通层的窗口
        if window.is_null()
            // 或者不是字典
            || unsafe { CFGetTypeID(window) != CFDictionaryGetTypeID() }
            // 窗口属于本进程
            || unsafe { number_i32(window, kCGWindowOwnerPID) } != Some(pid)
            // 或者窗口层级不为 0
            || unsafe { number_i32(window, kCGWindowLayer) } != Some(0)
        {
            // 不是则跳过
            continue;
        }
        // 透明度不可读则未知
        let alpha = unsafe { CFDictionaryGetValue(window, kCGWindowAlpha) };
        // 透明度缺失或类型不对
        if alpha.is_null() || unsafe { CFGetTypeID(alpha) != CFNumberGetTypeID() } {
            // 返回 None
            return None;
        }
        // 透明度缓冲
        let mut opacity = 0_f64;
        // kCFNumberFloat64Type = 6.
        // kCFNumberFloat64Type = 6
        if unsafe { CFNumberGetValue(alpha, 6, (&mut opacity as *mut f64).cast()) } == 0 {
            // 返回 None
            return None;
        }
        // 有可见窗口即为前台
        if opacity.is_finite() && opacity > 0.0 {
            // 有可见窗口
            return Some(true);
        }
    }
    // 没有可见窗口
    Some(false)
}
