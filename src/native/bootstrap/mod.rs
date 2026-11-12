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

/// 随连接存活的系统资源，跟随界面可见性：Android 持有多播锁，其余平台无需持有。
#[derive(Default)]
pub(super) struct Visibility(#[cfg(target_os = "android")] android::Multicast);

// 可见性守卫的方法
impl Visibility {
    /// 界面可见性变化。
    pub(super) fn set(&self, visible: bool) {
        #[cfg(target_os = "android")]
        self.0.set_visible(visible);
        #[cfg(not(target_os = "android"))]
        let _ = visible;
    }
}

#[cfg(target_os = "android")]
// Android 的小组件共享目录
pub(super) fn android_widget_directory() -> Option<std::path::PathBuf> {
    // 由 Android 引导实现给出
    android::widget_directory()
}

#[cfg(target_os = "android")]
// 通知 Android 系统小组件重绘
pub(super) fn android_widget_changed() {
    // 由 Android 引导实现派发
    android::widget_changed()
}

// 密钥记录格式，测试也编译
#[cfg(any(target_os = "android", test))]
#[path = "android/key_record.rs"]
mod android_key_record;
