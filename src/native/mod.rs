//! NSH 运行时模块:连接、安全凭据、引导与应用活动。
//! 该层向 NSH 暴露公开事实，配对秘密始终留在连接 owner 与安全存储。
// 各平台的启动配置
mod bootstrap;
// 配套设备（Apple Watch）通道
pub(crate) mod companion;
// 局域网连接 owner
mod connection;
// 系统安全存储
mod credentials;
// 持久化设备身份
mod identity;
// 图片解码
pub(crate) mod image;
// 系统前后台状态
mod lifecycle;
// 对 JS 暴露的连接模块
mod link;
// 对 JS 暴露的规则模块
mod rules;
// 对 JS 暴露的界面生命周期模块
mod ui_lifecycle;
// 系统小组件
pub(crate) mod widgets;

// 对外导出三个模块
// 真实连接的操作，由 Shell 调用
pub(crate) use link::Link;
// 导出规则
pub use rules::Rules;
// 导出外壳
pub use shell::Shell;
// 调试构建专用的测试夹具规则
#[cfg(debug_assertions)]
pub use rules::FixtureRules;
// 导出界面生命周期
pub use ui_lifecycle::UiLifecycle;

// 应用外壳控制器
mod shell;
