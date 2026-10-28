//! 配对页面和公开连接投影的业务入口；网络传输仍归原生 owner。
// 设备身份规则
pub(crate) mod identity;
// 快照投影
pub(crate) mod snapshot;
// 连接视图状态
pub(crate) mod view_state;

// 引入参数读取辅助函数
use crate::shared::{argument, text_argument};
// 引入 JSON 值与构造宏
use serde_json::{Value, json};

// 分发连接相关的规则查询，不属于本模块的操作返回 None
pub(crate) fn dispatch(op: &str, args: &[Value]) -> Option<Result<Value, String>> {
    // 按操作名匹配
    let result = match op {
        // 构造空视图：需要设备 ID 与名称
        "connection.emptyView" => text_argument(args, 0)
            // 取名称并构造空视图
            .and_then(|id| text_argument(args, 1).map(|name| view_state::empty_view(id, name))),
        // 解析原生端口返回的状态文本
        "connection.nativeView" => text_argument(args, 0).and_then(view_state::native_view),
        // 把快照投影为公开形式，校验失败返回 null
        "connection.parseSnapshot" => Ok(snapshot::project_snapshot(
            // 参数 0
            argument(args, 0),
            // 参数 1
            argument(args, 1),
        )
        // 缺省为 null
        .unwrap_or(Value::Null)),
        // 校验设备名称
        "connection.nameError" => {
            // 校验名称并返回问题
            text_argument(args, 0).map(|name| json!(identity::name_error(name)))
        }
        // 校验设备 ID
        "connection.validDeviceId" => {
            // 校验设备标识
            text_argument(args, 0).map(|id| json!(identity::valid_device_id(id)))
        }
        // 未知操作交给其他模块
        _ => return None,
    };
    // 返回结果
    Some(result)
}
