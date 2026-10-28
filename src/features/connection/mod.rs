//! 配对页面和公开连接投影的业务入口；网络传输仍归原生 owner。
// 设备身份规则
pub(crate) mod identity;
// 快照投影
pub(crate) mod snapshot;
// 连接视图状态
pub(crate) mod view_state;
