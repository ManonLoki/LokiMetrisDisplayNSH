//! 共享 UI 运行时入口；业务实现位于职责独立的 native 模块。
// 业务规则模块：连接、Hooks、设置与用量
mod features;
// 原生能力模块：连接、凭据、图片、生命周期等
mod native;
// 跨模块共享的契约与领域工具
mod shared;

// 对外导出三个原生模块入口
pub use native::{Rules, Shell, UiLifecycle};
// 引入 nsh 原生模块宏与注册表
use nsh_core::prelude::*;

// 声明原生模块名，运行时据此链接
#[native_module("loki-metris-mobile-display/native")]
// 注册所有原生绑定
fn register_bindings(registry: &mut Registry) {
    // 安装配套设备（手表）通道
    native::companion::install();
    // 安装小组件通道
    native::widgets::install();
    // 注册连接模块
    registry.module::<Shell>();
    // 注册界面生命周期模块
    registry.module::<UiLifecycle>();
    // 注册规则查询模块
    registry.module::<Rules>();
    // 调试构建额外注册测试夹具规则
    #[cfg(debug_assertions)]
    registry.module::<native::FixtureRules>();
    // 注册图片帧解码模块
    registry.module::<native::image::ImageFrames>();
}
