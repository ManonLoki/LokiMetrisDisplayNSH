//! 配对页面和公开连接投影的业务入口；网络传输仍归原生 owner。
pub(crate) mod identity;
pub(crate) mod snapshot;
pub(crate) mod view_state;

use crate::shared::{argument, text_argument};
use serde_json::{Value, json};

pub(crate) fn dispatch(op: &str, args: &[Value]) -> Option<Result<Value, String>> {
    let result = match op {
        "connection.emptyView" => text_argument(args, 0)
            .and_then(|id| text_argument(args, 1).map(|name| view_state::empty_view(id, name))),
        "connection.nativeView" => text_argument(args, 0).and_then(view_state::native_view),
        "connection.parseSnapshot" => Ok(snapshot::project_snapshot(
            argument(args, 0),
            argument(args, 1),
        )
        .unwrap_or(Value::Null)),
        "connection.nameError" => {
            text_argument(args, 0).map(|name| json!(identity::name_error(name)))
        }
        "connection.validDeviceId" => {
            text_argument(args, 0).map(|id| json!(identity::valid_device_id(id)))
        }
        _ => return None,
    };
    Some(result)
}
