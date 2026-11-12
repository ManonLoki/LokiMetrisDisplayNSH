//! iOS 宿主的启动通知与设备形态。

// iOS 的安装入口
#[cfg(target_os = "ios")]
#[path = "ios/mod.rs"]
mod platform;

#[cfg(target_os = "ios")]
// 重新导出安装与设备形态接口
pub(super) use platform::{form_factor, install};
