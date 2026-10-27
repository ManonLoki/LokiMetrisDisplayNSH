//! 共享 UI 运行时入口；业务实现位于职责独立的 native 模块。
mod app;
mod features;
mod native;
mod shared;

pub use native::{Link, Presentation, UiLifecycle};
use nsh_core::prelude::*;

#[native_module("loki-metris-mobile-display/native")]
fn register_bindings(registry: &mut Registry) {
    #[cfg(target_os = "ios")]
    native::companion::install();
    registry.module::<Link>();
    registry.module::<UiLifecycle>();
    registry.module::<Presentation>();
    registry.module::<native::image::ImageFrames>();
    registry.module::<app::Settings>();
}
