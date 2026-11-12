//! WidgetKit 的私有摘要存储、已认证的 HTTP 协议与显示投影。
//! 应用与扩展编译同一套应用快照与名称规则。
// 供 Swift 调用的 C 接口
pub mod ffi;
// 小组件私有摘要存储
pub mod store;
pub mod summary;
pub mod usage;
pub mod view;
// 带认证的 HTTP 协议
pub mod widget_http;

// 以下模块直接复用主应用的源文件，不维护第二份规则
#[path = "../../../src/shared/domain/agents.rs"]
pub mod agents;
#[path = "../../../src/shared/contracts/snapshot.rs"]
pub mod contracts;
#[path = "../../../src/features/connection/snapshot.rs"]
pub mod snapshot;
#[path = "../../../src/shared/domain/text.rs"]
pub mod text;

/// 规范的应用源模块,共享使用,无需第二个校验器。
// 主应用源码模块所需的路径别名，使共享文件无需改动即可编译
pub mod shared {
    pub mod domain {
        pub use crate::{agents, text};
    }
    pub mod contracts {
        pub use crate::contracts as snapshot;
    }
}
