//! NSH runtime modules for connection, secure credentials, bootstrap and application activity.
//! 该层向 NSH 暴露公开事实，配对秘密始终留在连接 owner 与安全存储。
mod bootstrap;
#[cfg(any(target_os = "ios", test))]
pub(crate) mod companion;
mod connection;
mod credentials;
mod exports;
mod identity;
pub(crate) mod image;
mod lifecycle;
mod presentation;

pub use exports::{Link, UiLifecycle};
pub use presentation::Presentation;
