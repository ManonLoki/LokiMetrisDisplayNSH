//! 不依赖页面或系统宿主的共同契约和 Agent 规则。
pub(crate) mod contracts;
pub(crate) mod domain;

use serde_json::Value;

pub(crate) fn argument(args: &[Value], index: usize) -> &Value {
    args.get(index).unwrap_or(&Value::Null)
}

pub(crate) fn text_argument(args: &[Value], index: usize) -> Result<&str, String> {
    argument(args, index)
        .as_str()
        .ok_or_else(|| "展示调用缺少文本参数".to_owned())
}

pub(crate) fn dispatch(op: &str, args: &[Value]) -> Option<Result<Value, String>> {
    let result = match op {
        "shared.defaultPanel" => Ok(contracts::panel::default_panel()),
        "shared.restorePanel" => Ok(contracts::panel::restore_panel(argument(args, 0).as_str())),
        "shared.canonicalAgentKey" => text_argument(args, 0)
            .map(|text| Value::String(domain::agents::canonical_agent_key(text))),
        "shared.agentDisplayName" => text_argument(args, 0)
            .map(|text| Value::String(domain::agents::agent_display_name(text))),
        _ => return None,
    };
    Some(result)
}
