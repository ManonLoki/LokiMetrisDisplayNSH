//! 为 NSH 自有运行时提供的最小系统引导;没有产品宿主,也没有系统扩展。
// 引入连接配置
use super::connection::Config;
// 引入路径
use std::path::Path;

// 平台启动实现
#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "ios")]
mod ios;

// 生成连接配置：移动端由各自平台读取身份与旧数据，桌面直接用参数
pub(super) fn configure(root: &Path, device_id: &str, name: &str) -> Result<Config, String> {
    #[cfg(target_os = "android")]
    return android::configure(root, device_id, name);
    #[cfg(target_os = "ios")]
    return ios::configure(root, device_id, name);
    // 桌面平台：由根目录、设备 ID 与名称直接构造
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    serde_json::from_value(serde_json::json!({"root":root,"deviceId":device_id,"name":name}))
        // 错误转为文本
        .map_err(|error| error.to_string())
}

// 输入形态：Android 可能是电视，其余为桌面
pub(super) fn form_factor() -> String {
    #[cfg(target_os = "android")]
    return android::device_form();
    #[cfg(not(target_os = "android"))]
    "desktop".into()
}

/// 只要 Link 持有它,就让系统活动观察一直保持存活。
#[cfg(target_os = "ios")]
// 守卫类型：iOS 持有观察者，其余平台无需持有
pub(super) type ActivityGuard = super::lifecycle::ActivityObserver;
#[cfg(not(target_os = "ios"))]
pub(super) type ActivityGuard = ();

/// 开始向所有者推送系统前台事实;桌面端则改为轮询。
// 开始把系统前后台事实推送给 owner；桌面平台改为轮询
pub(super) fn observe(handle: std::sync::Weak<super::connection::Handle>) -> ActivityGuard {
    #[cfg(target_os = "ios")]
    return ios::observe(handle);
    #[cfg(target_os = "android")]
    return android::observe(handle);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    drop(handle)
}

#[cfg(target_os = "android")]
// Android 的前台状态
pub(super) fn foreground() -> Option<bool> {
    // 取 Android 前台状态
    android::foreground()
}

// 密钥记录格式，测试也编译
#[cfg(any(target_os = "android", test))]
#[path = "android/key_record.rs"]
mod android_key_record;
