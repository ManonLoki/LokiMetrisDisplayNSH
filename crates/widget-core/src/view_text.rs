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

pub(super) fn widget_token(amount: Option<&Value>, has_view: bool) -> Value {
    let precise = amount.map_or("等待桌面数据", |value| string(&value["text"]));
    let quality = match amount {
        None if !has_view => "点击打开应用并配对",
        None => "Token 暂不可用",
        Some(value) if !string(&value["quality"]).is_empty() => string(&value["quality"]),
        Some(value) if !value["exact"].is_null() => "完整 · 精确",
        Some(_) => precise,
    };
    json!({
        "precise": precise,
        "amount": amount.map_or(if has_view { "不可用" } else { "—" }, |value| string(&value["compactWide"])),
        "quality": quality,
        "amountDescription": format!("Token 用量：{precise}，{quality}"),
        "exactDescription": format!("精确 Token 用量：{precise}"),
    })
}

pub(super) fn describe(request: &Value) -> Value {
    let view = &request["view"];
    let has_view = view.is_object();
    let tokens = &view["tokens"];
    let amount = (!tokens.is_null()).then_some(&tokens["total"]);
    let widget = widget_token(amount, has_view);
    let sync = request["syncTime"].as_str().filter(|text| !text.is_empty());
    let compact_sync = request["compactSyncTime"]
        .as_str()
        .filter(|text| !text.is_empty());
    let quota_time = request["quotaTime"]
        .as_str()
        .filter(|text| !text.is_empty());
    let quota_compact_time = request["quotaCompactTime"]
        .as_str()
        .filter(|text| !text.is_empty());
    let hidden_agents = request["visibleAgents"].as_u64().and_then(|visible| {
        let count = view["agents"].as_array()?.len() as u64;
        count.checked_sub(visible).filter(|count| *count > 0)
    });
    let day = if tokens.is_object() {
        string(&tokens["dayDescription"])
    } else {
        "统计日期待同步"
    };
    let quota = string(&view["quota"]["description"]);
    let hooks = if has_view {
        string(&view["hooksDescription"])
    } else {
        "Hooks 待同步"
    };
    let sync_text = sync.map_or_else(
        || "尚未同步".to_owned(),
        |stamp| format!("上次同步 {stamp}"),
    );
    let sync_description = sync.map_or_else(
        || "尚未同步".to_owned(),
        |stamp| format!("最近同步 {stamp}，显示最近一次同步的数据"),
    );
    let app_name = request["appName"].as_str().unwrap_or("诡秘神谕");
    let mut root = format!("{app_name}，{}。", string(&widget["amountDescription"]));
    if tokens.is_object() {
        root.push_str(&format!("{day}。"));
    }
    if !quota.is_empty() {
        root.push_str(&format!("{quota}。"));
    }
    root.push_str(hooks);
    root.push('。');
    root.push_str(&sync.map_or_else(
        || "尚未同步。".to_owned(),
        |stamp| format!("最近同步 {stamp}。"),
    ));
    root.push_str("点击打开应用查看详情。");
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
    let compact_stamp = compact_sync.or(sync);
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
    json!({
        "widgetEmptyInline": format!("{app_name} · 尚未同步"),
        "widgetEmptyCircular": "未同步", "widgetEmptyTitle": "尚未同步",
        "widgetEmptyHelp": "打开 App 连接桌面",
        "watchWaitingTitle": "等待 iPhone 同步", "watchQuotaWaitingTitle": "等待额度数据",
        "watchWaitingHelp": "在 iPhone 中连接桌面后自动同步",
        "widgetToken": widget, "tokenMessage": (!has_view).then_some("点击打开应用并配对"),
        "hooksWidgetFallback": if has_view { string(&view["hooksWidgetFallback"]) } else { "Hooks 待同步" },
        "hookPlaceholder": if has_view { string(&view["hookPlaceholder"]) } else { "等待 iPhone 同步" },
        "quotaDescription": quota, "dayDescription": day,
        "syncText": sync_text,
        "compactSyncText": compact_sync.map_or_else(|| "尚未同步".to_owned(), |stamp| format!("上次 {stamp}")),
        "syncDescription": sync_description, "rootDescription": root,
        "quotaUpdate": quota_time.map_or_else(String::new, |stamp| format!("额度更新于 {stamp}")),
        "quotaCompactUpdate": quota_compact_time.map_or_else(String::new, |stamp| format!("更新 {stamp}")),
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
