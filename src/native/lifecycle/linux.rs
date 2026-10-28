// 引入 C 类型
use std::ffi::{c_int, c_void};
// 引入原子类型
use std::sync::{
    // 一次性初始化
    OnceLock,
    // 原子类型
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};
// 引入时钟
use std::time::Instant;

// Linux 宿主已经链接了这些 GTK 4 系统库。GTK 控件
// 只在默认 GTK 主上下文的空闲回调中读取;
// 引擎线程只负责挂接该事件源并读取原子量。
// GTK 控件只在默认主上下文的空闲回调里读取；引擎线程只挂接回调并读取原子量
#[link(name = "gtk-4")]
unsafe extern "C" {
    // 顶层窗口列表
    fn gtk_window_get_toplevels() -> *mut c_void;
    // 窗口是否激活
    fn gtk_window_is_active(window: *mut c_void) -> c_int;
    // 窗口是否已映射
    fn gtk_widget_get_mapped(widget: *mut c_void) -> c_int;
}
#[link(name = "gio-2.0")]
unsafe extern "C" {
    // 列表项数
    fn g_list_model_get_n_items(model: *mut c_void) -> u32;
    // 取第 n 项（转移所有权）
    fn g_list_model_get_item(model: *mut c_void, position: u32) -> *mut c_void;
}
#[link(name = "gobject-2.0")]
unsafe extern "C" {
    // 释放引用
    fn g_object_unref(object: *mut c_void);
}
#[link(name = "glib-2.0")]
unsafe extern "C" {
    // 向默认主上下文添加空闲回调（线程安全）
    fn g_idle_add_full(
        // 优先级
        priority: c_int,
        // 回调函数
        function: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
        // 回调数据
        data: *mut c_void,
        // 销毁通知
        notify: Option<unsafe extern "C" fn(*mut c_void)>,
        // 返回源标识
    ) -> u32;
}

// 0 = 未知,1 = 没有活动的已映射窗口,2 = 有活动的已映射窗口。
// 0 未知，1 没有激活的已映射窗口，2 有
static STATE: AtomicU8 = AtomicU8::new(0);
// 是否已有一次采样在排队
static PENDING: AtomicBool = AtomicBool::new(false);
// 最近一次采样完成的时刻
static UPDATED_MS: AtomicU64 = AtomicU64::new(0);
// 时钟起点
static START: OnceLock<Instant> = OnceLock::new();

// 毫秒时钟；加 1 为“GTK 回调从未运行”保留 0
fn clock_ms() -> u64 {
    // 偏移量让零保留给“GTK 回调从未运行过”。
    START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

// 在 GTK 线程上采样所有顶层窗口
unsafe extern "C" fn sample_on_gtk_thread(_: *mut c_void) -> c_int {
    // 状态默认为 0
    let mut state = 0;
    // gtk_window_get_toplevels() 是 transfer-none。GListModel 返回的
    // 每一项都是 transfer-full,必须取消引用一次。
    // 列表本身不转移所有权，取出的每一项需各释放一次
    let windows = unsafe { gtk_window_get_toplevels() };
    // 窗口列表非空
    if !windows.is_null() {
        // 窗口数量
        let count = unsafe { g_list_model_get_n_items(windows) };
        // 数量异常时视为未知
        if count <= 10_000 {
            // 先记为后台
            state = 1;
            // 逐个检查
            for index in 0..count {
                // 取窗口
                let window = unsafe { g_list_model_get_item(windows, index) };
                // 取不到则结果未知
                if window.is_null() {
                    // 取不到则状态未知
                    state = 0;
                    // 继续下一个
                    continue;
                }
                // 激活且已映射才算前台
                let active = unsafe {
                    // 窗口活跃且已映射
                    gtk_window_is_active(window) != 0 && gtk_widget_get_mapped(window) != 0
                };
                // 释放引用
                unsafe { g_object_unref(window) };
                // 找到即可结束
                if active {
                    // 记为前台
                    state = 2;
                    // 结束
                    break;
                }
            }
        }
    }
    // 记录结果与时间，并允许下一次采样
    STATE.store(state, Ordering::Relaxed);
    // 记录更新时刻
    UPDATED_MS.store(clock_ms(), Ordering::Release);
    // 清除待采样标记
    PENDING.store(false, Ordering::Release);
    // 每次轮询最多排一次新采样，回调执行后即移除
    0 // G_SOURCE_REMOVE:每次轮询最多安排一次新的采样。
}

// 前台判断：只读原子量，必要时排一次采样
pub(super) fn foreground() -> Option<bool> {
    // 没有排队中的采样才新增
    if !PENDING.swap(true, Ordering::AcqRel) {
        // g_idle_add_full 是线程安全的,总是挂接到全局默认上下文。
        // 与 MainContext::invoke 不同,它不会在主循环尚未获取 GTK 之前,
        // 就在引擎线程上内联调用 GTK。
        // 这个函数始终挂到全局默认上下文，不会在主循环就绪前于引擎线程内联调用 GTK
        let source =
            // 在 GTK 线程上安排采样
            unsafe { g_idle_add_full(200, Some(sample_on_gtk_thread), std::ptr::null_mut(), None) };
        // 挂接失败
        if source == 0 {
            // 清除待采样标记
            PENDING.store(false, Ordering::Release);
            // 返回 None
            return None;
        }
    }
    // 上次更新时刻
    let updated = UPDATED_MS.load(Ordering::Acquire);
    // 从未采样或超过 2 秒未更新：GTK 循环可能停了，不能沿用旧的 true
    if updated == 0 || clock_ms().saturating_sub(updated) > 2_000 {
        // 已停止或停滞的 GTK 循环不得让旧的 true 值一直存活。
        return None;
    }
    // 按状态返回
    match STATE.load(Ordering::Relaxed) {
        // 2 表示后台
        1 => Some(false),
        // 前台
        2 => Some(true),
        // 未知
        _ => None,
    }
}
