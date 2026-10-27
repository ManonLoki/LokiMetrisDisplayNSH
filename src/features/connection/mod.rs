//! 配对页面和公开连接投影的业务入口；网络传输仍归原生 owner。
pub(crate) mod identity;
pub(crate) mod model;
pub(crate) mod snapshot;

use crate::shared::{argument, text_argument};
use serde_json::{Value, json};

pub(crate) fn dispatch(op: &str, args: &[Value]) -> Option<Result<Value, String>> {
    let result = match op {
        "connection.emptyView" => text_argument(args, 0)
            .and_then(|id| text_argument(args, 1).map(|name| model::empty_view(id, name))),
        "connection.nativeView" => {
            text_argument(args, 0).and_then(|raw| model::native_view(raw, argument(args, 1)))
        }
        "connection.parseSnapshot" => Ok(snapshot::parse_snapshot(
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
        "connection.utf8Bytes" => text_argument(args, 0).map(|text| json!(text.len())),
        "connection.commitDeviceName" => text_argument(args, 0).and_then(|name| {
            text_argument(args, 1).map(|id| identity::commit_device_name(name, id))
        }),
        "connection.maxNameBytes" => Ok(json!(identity::MAX_NAME_BYTES)),
        "connection.zeroDeviceId" => Ok(json!(identity::ZERO_DEVICE_ID)),
        _ => return None,
    };
    Some(result)
}
