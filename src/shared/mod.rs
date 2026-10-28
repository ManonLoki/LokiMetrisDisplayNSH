//! 不依赖页面或系统宿主的共同契约和 Agent 规则。
// 数据契约
pub(crate) mod contracts;
// 领域规则
pub(crate) mod domain;

// 引入 JSON 值
use serde_json::Value;

// 取第 index 个参数，缺失视为 null
pub(crate) fn argument(args: &[Value], index: usize) -> &Value {
    // 取参数，缺失为 null
    args.get(index).unwrap_or(&Value::Null)
}

// 取文本参数，不是文本则报错
pub(crate) fn text_argument(args: &[Value], index: usize) -> Result<&str, String> {
    // 取参数
    argument(args, index)
        // 取字符串
        .as_str()
        // 缺少则报缺少文本参数
        .ok_or_else(|| "展示调用缺少文本参数".to_owned())
}

// 分发共享规则查询
pub(crate) fn dispatch(op: &str, args: &[Value]) -> Option<Result<Value, String>> {
    // 按操作名分发
    let result = match op {
        // 默认面板配置
        "shared.defaultPanel" => Ok(contracts::panel::default_panel()),
        // 从存储文本恢复面板配置
        "shared.restorePanel" => Ok(contracts::panel::restore_panel(argument(args, 0).as_str())),
        // 代理名归一
        "shared.canonicalAgentKey" => text_argument(args, 0)
            // 规范代理键
            .map(|text| Value::String(domain::agents::canonical_agent_key(text))),
        // 代理显示名
        "shared.agentDisplayName" => text_argument(args, 0)
            // 代理显示名
            .map(|text| Value::String(domain::agents::agent_display_name(text))),
        // 其余操作不支持
        _ => return None,
    };
    // 返回结果
    Some(result)
}
