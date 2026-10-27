//! 展示模型中的描述模板；日期只接受宿主已按本机区域格式化的公开文字。
use serde_json::{Value, json};

pub(super) fn string(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

pub(super) fn qualified(text: &str, quality: &str) -> String {
    if quality.is_empty() {
        text.to_owned()
    } else {
        format!("{text}，{quality}")
    }
}

pub(super) fn describe(request: &Value) -> Value {
    let view = &request["view"];
    let tokens = &view["tokens"];
    let stamp = |key: &str| request[key].as_str().filter(|text| !text.is_empty());
    let sync = stamp("syncTime");
    let quota_time = stamp("quotaTime");
    let hidden_agents = request["visibleAgents"].as_u64().and_then(|visible| {
        let count = view["agents"].as_array()?.len() as u64;
        count.checked_sub(visible).filter(|count| *count > 0)
    });
    let provider_descriptions: serde_json::Map<String, Value> = view["quota"]["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|provider| {
            let description = string(&provider["description"]);
            let description = quota_time.map_or_else(
                || description.to_owned(),
                |stamp| format!("{description}，额度更新于 {stamp}"),
            );
            (string(&provider["provider"]).to_owned(), json!(description))
        })
        .collect();
    let apple_sync = sync.map_or_else(
        || "尚未同步".to_owned(),
        |stamp| format!("上次同步于 {stamp}"),
    );
    let compact_stamp = stamp("compactSyncTime").or(sync);
    let compact_description = if tokens.is_object() {
        format!("{}，{apple_sync}", string(&tokens["dateAccessibility"]))
    } else {
        apple_sync.clone()
    };
    let compact_text = compact_stamp.map_or_else(
        || "尚未同步".to_owned(),
        |stamp| {
            if tokens.is_object() {
                format!("{} · 同步 {stamp}", string(&tokens["date"]))
            } else {
                format!("同步 {stamp}")
            }
        },
    );
    let app_name = request["appName"].as_str().unwrap_or("诡秘神谕");
    json!({
        "widgetEmptyInline": format!("{app_name} · 尚未同步"),
        "widgetEmptyCircular": "未同步", "widgetEmptyTitle": "尚未同步",
        "widgetEmptyHelp": "打开 App 连接桌面",
        "quotaUpdate": quota_time.map_or_else(String::new, |stamp| format!("额度更新于 {stamp}")),
        "quotaCompactUpdate": stamp("quotaCompactTime").map_or_else(String::new, |stamp| format!("更新 {stamp}")),
        "remainingAgents": hidden_agents.map_or_else(String::new, |count| format!("另 {count} 个 Agent")),
        "providerDescriptions": provider_descriptions,
        "appleSyncText": compact_stamp.map_or_else(|| "尚未同步".to_owned(), |stamp| format!("上次同步 {stamp}")),
        "appleSyncDescription": apple_sync,
        "appleCompactSyncText": compact_text, "appleCompactSyncDescription": compact_description,
        "appleAccessoryInline": compact_stamp.map_or_else(|| string(&view["hooksText"]).to_owned(), |stamp| format!("上次 {stamp} · {}", string(&view["hooksText"]))),
        "appleAccessoryCircular": format!("{apple_sync}，{}", string(&view["hooksText"])),
    })
}

#[cfg(test)]
#[path = "view_text_test.rs"]
mod view_text_test;
