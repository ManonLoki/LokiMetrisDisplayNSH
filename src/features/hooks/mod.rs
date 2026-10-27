//! Hook 状态属于业务事实；页码、单元格和详情展开仍由 UI 控制。
use serde_json::{Value, json};

pub(crate) fn dispatch(op: &str, args: &[Value]) -> Option<Result<Value, String>> {
    (op == "hooks.label").then(|| {
        Ok(json!(match args.first().and_then(Value::as_str) {
            Some("idle") => "空闲",
            Some("running") => "运行",
            Some("asking") => "询问",
            _ => "出错",
        }))
    })
}
