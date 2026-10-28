//! WidgetKit 系统访问;只有已认证的所有者才能发布私有读取目标。
// 仅 iOS：引入 C 字符串与路径类型
#[cfg(target_os = "ios")]
use std::{
    // C 字符串类型
    ffi::{CStr, c_char},
    // 路径类型
    path::PathBuf,
};

#[cfg(target_os = "ios")]
// Swift 垫片导出的函数
unsafe extern "C" {
    // 获取 App Group 目录
    fn loki_widget_host_directory() -> *mut c_char;
    // 释放垫片分配的字符串
    fn loki_widget_host_free(value: *mut c_char);
    // 请求刷新小组件
    fn loki_widget_host_reload();
    // 注册系统入口
    fn loki_widget_host_bootstrap();
}

/// 安装进程的系统刷新入口,不会另外创建局域网所有者。
// 安装进程级的系统刷新入口，不创建另一个局域网 owner
pub(crate) fn install() {
    #[cfg(target_os = "ios")]
    {
        // 最终的 Swift 宿主对象会引用这些 C 导出。在 Rust LTO 下
        // 保留它们的地址,但不执行任何一个入口。
        // 最终的 Swift 宿主引用这些 C 导出；这里保留其地址使 LTO 不会裁掉，但不执行
        let call: unsafe extern "C" fn(*const c_char) -> *mut c_char =
            // 引入展示核心调用函数
            loki_widget_core::ffi::loki_core_call;
        // 导出释放函数
        let free: unsafe extern "C" fn(*mut c_char) = loki_widget_core::ffi::loki_core_free;
        // 防止链接器丢弃这两个符号
        std::hint::black_box((call, free));
        // SAFETY: 静态链接的 Swift 垫片持有其幂等的系统注册。
        // 垫片自行保证注册幂等
        unsafe { loki_widget_host_bootstrap() };
    }
}

/// 只接受原生垫片返回的、系统授予的 App Group 目录。
#[cfg(target_os = "ios")]
// 只接受系统授予的 App Group 目录
pub(super) fn directory() -> Option<PathBuf> {
    // SAFETY: 垫片返回的是持有所有权、以 NUL 结尾的分配,或 null。
    // 垫片返回自有的 NUL 结尾字符串，或空指针
    let raw = unsafe { loki_widget_host_directory() };
    // 指针为空
    if raw.is_null() {
        // 返回 None
        return None;
    }
    // SAFETY: 在下面匹配的垫片 free 之前,raw 一直有效。
    // 转为路径
    let path = unsafe { CStr::from_ptr(raw) }
        // 转为字符串
        .to_str()
        // 出错忽略
        .ok()
        // 转为路径
        .map(PathBuf::from);
    // SAFETY: 这是垫片的分配,且只释放一次。
    // 释放垫片分配的内存，仅一次
    unsafe { loki_widget_host_free(raw) };
    // 必须是已存在的绝对目录
    path.filter(|path| path.is_absolute() && path.is_dir())
}

#[cfg(any(target_os = "ios", test))]
#[derive(Default)]
// 刷新节流门
struct ReloadGate {
    // 上次刷新时刻
    last: Option<std::time::Instant>,
    // 是否已有等待中的补刷
    pending: bool,
}

#[cfg(any(target_os = "ios", test))]
// 重载门的方法
impl ReloadGate {
    // 请求刷新：返回需要等待的时长，None 表示已有补刷在排队
    fn request(&mut self, now: std::time::Instant) -> Option<std::time::Duration> {
        // 已有待执行的重载
        if self.pending {
            // 不再安排
            return None;
        }
        // 最小间隔 30 秒
        let interval = std::time::Duration::from_secs(30);
        // 距离下次允许刷新还要多久
        let remaining = self
            // 上次重载时刻
            .last
            // 距上次的间隔不足则需等待
            .and_then(|last| interval.checked_sub(now.saturating_duration_since(last)))
            // 否则无需等待
            .unwrap_or_default();
        // 立即刷新并记录时刻
        if remaining.is_zero() {
            // 记录本次重载时刻
            self.last = Some(now);
        // 否则登记一次补刷
        } else {
            // 标记有待执行的重载
            self.pending = true;
        }
        // 返回需要等待的时间
        Some(remaining)
    }
}

/// 合并摘要变更触发的刷新,每三十秒保留一次尾随更新。
// 摘要变化时合并刷新请求，每 30 秒最多保留一次补刷
pub(super) fn changed() {
    #[cfg(target_os = "ios")]
    {
        // 标准库同步原语
        use std::{
            // 互斥锁与一次性初始化
            sync::{Mutex, OnceLock},
            // 时刻
            time::Instant,
        };
        // 全局节流门
        static GATE: OnceLock<Mutex<ReloadGate>> = OnceLock::new();
        // 全局重载门
        let gate = GATE.get_or_init(|| Mutex::new(ReloadGate::default()));
        // 计算需要延后的时间
        let delay = gate
            // 加锁
            .lock()
            // 锁中毒则崩溃
            .expect("widget reload gate poisoned")
            // 申请一次重载
            .request(Instant::now());
        // 已有补刷排队，直接返回
        let Some(delay) = delay else {
            // 无需再安排
            return;
        };
        // 无需等待：立即刷新
        if delay.is_zero() {
            // SAFETY: 原生垫片把 WidgetKit 访问派发到系统队列。
            // 垫片会把 WidgetKit 访问派发到系统队列
            unsafe { loki_widget_host_reload() };
        // 需要等待：在主队列上延后刷新
        } else if let Ok(when) = dispatch2::DispatchTime::try_from(delay) {
            // 在主队列上延后执行
            let _ = dispatch2::DispatchQueue::main().after(when, move || {
                // 回调中清除排队标记并记录刷新时刻
                let mut state = gate.lock().expect("widget reload gate poisoned");
                // 清除待执行标记
                state.pending = false;
                // 记录本次重载时刻
                state.last = Some(Instant::now());
                // 释放锁
                drop(state);
                // SAFETY: 回调在主队列上运行,且垫片与进程同寿命。
                // 回调在主队列执行，垫片与进程同寿命
                unsafe { loki_widget_host_reload() };
            });
        }
    }
}

// 测试：刷新被合并且保留末尾那次变化
#[cfg(test)]
mod tests {
    // 引入上层
    use super::*;
    // 时长与时刻
    use std::time::{Duration, Instant};
    #[test]
    fn reloads_coalesce_and_preserve_the_trailing_change() {
        // 当前时刻
        let now = Instant::now();
        // 新建重载门
        let mut gate = ReloadGate::default();
        // 首次立即刷新
        assert_eq!(gate.request(now), Some(Duration::ZERO));
        // 5 秒后再请求：需要再等 25 秒
        assert_eq!(
            // 申请：五秒后
            gate.request(now + Duration::from_secs(5)),
            // 需再等 25 秒
            Some(Duration::from_secs(25))
        );
        // 已有补刷排队，其余请求被合并
        assert_eq!(gate.request(now + Duration::from_secs(6)), None);
        // 模拟补刷已执行
        gate.pending = false;
        // 模拟三十秒后的上次时刻
        gate.last = Some(now + Duration::from_secs(30));
        // 又过 30 秒后可立即刷新
        assert_eq!(
            // 申请：一分钟后
            gate.request(now + Duration::from_secs(60)),
            // 无需等待
            Some(Duration::ZERO)
        );
    }
}
