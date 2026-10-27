//! 共享 UI 运行时入口；业务实现位于职责独立的 native 模块。
mod features;
mod native;
mod shared;

pub use native::{Link, Rules, UiLifecycle};
use nsh_core::prelude::*;

#[native_module("loki-metris-mobile-display/native")]
fn register_bindings(registry: &mut Registry) {
    native::companion::install();
    registry.module::<Link>();
    registry.module::<UiLifecycle>();
    registry.module::<Rules>();
    registry.module::<native::image::ImageFrames>();
    registry.module::<features::settings::Settings>();
}
