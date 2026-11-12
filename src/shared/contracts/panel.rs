//! 偏好恢复只采用已知枚举值；损坏的旧存储回到可用的默认布局。
use serde_json::{Value, json};

// 默认配置：两项都显示，用量精简，Hooks 每页一张
pub(crate) fn default_panel() -> Value {
    json!({"showUsage": true, "showHooks": true, "usageDetail": "compact", "hookLayout": "1x1"})
}

// 从存储文本恢复，解析失败回落默认值
pub(crate) fn restore_panel(raw: Option<&str>) -> Value {
    // 解析为 JSON，失败为 None
    raw.and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .as_ref()
        .map(panel_from_value)
        .unwrap_or_else(default_panel)
}

// 只采用已知枚举值：未知输入回落到默认
pub(crate) fn panel_from_value(value: &Value) -> Value {
    json!({
        // 仅明确为 false 才关闭
        "showUsage": value["showUsage"] != false,
        // 显示 Hook，缺省为是
        "showHooks": value["showHooks"] != false,
        // 只有 "detailed" 才是详细
        "usageDetail": if value["usageDetail"] == "detailed" { "detailed" } else { "compact" },
        // 只有 "2x2" 才是四宫格
        "hookLayout": if value["hookLayout"] == "2x2" { "2x2" } else { "1x1" },
    })
}
