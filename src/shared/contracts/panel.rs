//! 偏好恢复只采用已知枚举值；损坏的旧存储回到可用的默认布局。
use serde_json::{Value, json};

pub(crate) fn default_panel() -> Value {
    json!({"showUsage": true, "showHooks": true, "usageDetail": "compact", "hookLayout": "1x1"})
}

pub(crate) fn restore_panel(raw: Option<&str>) -> Value {
    raw.and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .as_ref()
        .map(panel_from_value)
        .unwrap_or_else(default_panel)
}

pub(crate) fn panel_from_value(value: &Value) -> Value {
    json!({
        "showUsage": value["showUsage"] != false,
        "showHooks": value["showHooks"] != false,
        "usageDetail": if value["usageDetail"] == "detailed" { "detailed" } else { "compact" },
        "hookLayout": if value["hookLayout"] == "2x2" { "2x2" } else { "1x1" },
    })
}
