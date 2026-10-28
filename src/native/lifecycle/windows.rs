// 引入不透明指针类型
use std::ffi::c_void;
// 链接 user32
#[link(name = "user32")]
unsafe extern "system" {
    // 前台窗口
    fn GetForegroundWindow() -> *mut c_void;
    // 窗口所属进程
    fn GetWindowThreadProcessId(window: *mut c_void, pid: *mut u32) -> u32;
    // 是否可见
    fn IsWindowVisible(window: *mut c_void) -> i32;
    // 是否最小化
    fn IsIconic(window: *mut c_void) -> i32;
}

// 前台判断：前台窗口属于本进程、可见且未最小化
pub(super) fn foreground() -> Option<bool> {
    // 前台窗口
    let window = unsafe { GetForegroundWindow() };
    // 激活切换期间可能短暂为空，结果未知
    if window.is_null() {
        // Windows 文档指出,激活状态变化期间会短暂出现 NULL。
        return None;
    }
    // 进程号
    let mut pid = 0;
    // 取进程 ID 失败则未知
    if unsafe { GetWindowThreadProcessId(window, &mut pid) } == 0 || pid == 0 {
        // 返回 None
        return None;
    }
    // 前台窗口属于别的进程
    if pid != std::process::id() {
        // 不是本进程的窗口
        return Some(false);
    }
    // 可见且未最小化
    Some(unsafe { IsWindowVisible(window) != 0 && IsIconic(window) == 0 })
}
