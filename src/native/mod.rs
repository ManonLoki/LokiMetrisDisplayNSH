//! NSH runtime modules for connection, secure credentials, bootstrap and application activity.
//! 该层向 NSH 暴露公开事实，配对秘密始终留在连接 owner 与安全存储。
mod bootstrap;
pub(crate) mod companion;
mod connection;
mod credentials;
mod identity;
pub(crate) mod image;
mod lifecycle;
mod link;
mod rules;
mod ui_lifecycle;
pub(crate) mod widgets;

pub use link::Link;
pub use rules::Rules;
pub use ui_lifecycle::UiLifecycle;
