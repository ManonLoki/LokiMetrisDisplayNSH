//! iOS 宿主的启动通知。

// iOS 的安装入口
#[cfg(target_os = "ios")]
#[path = "ios/mod.rs"]
mod platform;

#[cfg(target_os = "ios")]
pub(super) use platform::install;
