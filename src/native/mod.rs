//! NSH 运行时模块:连接、安全凭据、引导与应用活动。
//! 该层向 NSH 暴露公开事实，配对秘密始终留在连接 owner 与安全存储。
mod bootstrap;
// 配套设备（Apple Watch）通道
pub(crate) mod companion;
// 局域网连接 owner
mod connection;
mod credentials;
mod identity;
pub(crate) mod image;
mod lifecycle;
// 对 JS 暴露的连接模块
mod link;
// 对 JS 暴露的规则模块
mod rules;
pub(crate) mod widgets;

// 真实连接的操作，由 Shell 调用
pub(crate) use link::Link;
pub use rules::Rules;
pub use shell::Shell;

mod shell;
// 调试构建专用的测试夹具连接
#[cfg(debug_assertions)]
mod fixture;
