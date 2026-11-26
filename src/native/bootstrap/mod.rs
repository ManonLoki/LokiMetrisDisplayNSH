//! 为 NSH 自有运行时提供的最小系统引导;没有产品宿主,也没有系统扩展。
use std::path::{Path, PathBuf};

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "ios")]
mod ios;

/// 原生引导提供的连接配置。移动端使用系统身份。
pub(super) struct Config {
    /// 应用私有目录，owner 在其下建立 loki-link-v1。
    pub root: PathBuf,
    pub device_id: String,
    pub name: String,
    pub host_identity: bool,
    pub platform: String,
    pub watch_companion: bool,
    /// 由系统授予的小组件共享目录。
    pub widget_dir: Option<PathBuf>,
    pub app_version: String,
    /// phone 或 tablet，桌面不上报形态。
    pub form_factor: Option<String>,
    pub keep_snapshot: bool,
}

impl Config {
    fn new(root: PathBuf, device_id: String, name: String) -> Self {
        Self {
            root,
            device_id,
            name,
            host_identity: false,
            platform: std::env::consts::OS.into(),
            watch_companion: false,
            widget_dir: None,
            app_version: if cfg!(target_os = "ios") {
                "1.0"
            } else {
                "1.0.0"
            }
            .into(),
            form_factor: None,
            keep_snapshot: true,
        }
    }
}

// 生成连接配置：移动端由各自平台读取身份与旧数据，桌面直接用参数
pub(super) fn configure(root: &Path, device_id: &str, name: &str) -> Result<Config, String> {
    #[cfg(target_os = "android")]
    return android::configure(root, device_id, name);
    #[cfg(target_os = "ios")]
    return ios::configure(root, device_id, name);
    // 桌面平台：由根目录、设备 ID 与名称直接构造
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    Ok(Config::new(root.to_owned(), device_id.into(), name.into()))
}

/// 随连接存活的系统资源，跟随界面可见性：Android 持有多播锁，其余平台无需持有。
#[derive(Default)]
pub(super) struct Visibility(#[cfg(target_os = "android")] android::Multicast);

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
