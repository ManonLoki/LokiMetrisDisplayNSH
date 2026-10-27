//! 系统扩展的展示投影：所有文字、排序、筛选与阈值都在这里决定，
//! SwiftUI、Watch 与 Android RemoteViews 只按字段绘制，日期按本机区域格式化。
use crate::{
    agents::agent_display_name,
    usage::{agents, formatting, quota},
};
use serde_json::{Value, json};

#[path = "view_text.rs"]
mod text;

/// 系统先按本机区域格式化日期，核心再生成包含日期的完整描述。
pub fn describe(request: &Value) -> Value {
    text::describe(request)
}

/// 输入为已校验的摘要快照，或手机转发给手表的完整快照（Hook 可带图片摘要）。
pub fn project(snapshot: &Value) -> Value {
    let enabled = enabled(snapshot);
    let tokens = &snapshot["tokens"];
    let token_error = !snapshot["tokenError"].is_null();
    let mut hooks: Vec<_> = snapshot["hooks"].as_array().into_iter().flatten().collect();
    hooks.sort_by_key(|hook| hook["slot"].as_u64());
    let hooks: Vec<_> = hooks
        .into_iter()
        .map(|hook| {
            let agent = hook["agent"].as_str().unwrap_or_default();
            let name = agent_display_name(agent);
            let label = behavior_label(hook["behavior"].as_str());
            json!({
                "agent": agent, "name": name, "slot": hook["slot"],
                "behavior": hook["behavior"], "label": label,
                "accessibilityLabel": format!("{name}，{label}"),
                "imagePlaceholder": if hook["image"].is_null() { "暂无图片" } else { "图片未缓存" },
                "image": hook["image"],
            })
        })
        .collect();
    let available = snapshot["hooksAvailable"] == true;
    let counts: Vec<_> = ["running", "asking", "error", "idle"]
        .iter()
        .map(|state| {
            let count = hooks.iter().filter(|hook| hook["behavior"] == *state).count();
            let label = behavior_label(Some(state));
            json!({ "behavior": state, "label": label, "count": count, "text": format!("{label} {count}") })
        })
        .collect();
    let providers: Vec<_> = snapshot["providerQuota"]["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|provider| {
            agents::is_enabled(
                provider["provider"].as_str().unwrap_or_default(),
                enabled.as_deref(),
            )
        })
        .map(provider)
        .collect();
    let hook_summary = if available {
        format!("上次 Hooks {}", hooks.len())
    } else {
        "Hooks 未同步".into()
    };
    let hooks_description = counts
        .iter()
        .map(|item| item["text"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" · ");
    let amount = (!tokens.is_null()).then(|| amount(&tokens["all"]));
    let widget_token = text::widget_token(amount.as_ref(), true);
    let quota_description = providers
        .iter()
        .map(|provider| provider["widgetDescription"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("；");
    json!({
        "hooksAvailable": available,
        "hookCount": hooks.len(),
        "hookCountText": if available { hooks.len().to_string() } else { "—".into() },
        "hooksText": if available { format!("Hooks {}", hooks.len()) } else { "Hooks 未同步".into() },
        "hooksSummary": hook_summary,
        "hooksDescription": if available { hooks_description } else { "Hooks 暂不可用".into() },
        "hooksWidgetFallback": "Hooks 暂不可用",
        "hookPlaceholder": if available { "暂无活动" } else { "暂不可用" },
        "hookLead": hooks.first().map(|hook| format!("· {} {}", text::string(&hook["name"]), text::string(&hook["label"]))),
        "behaviorCounts": counts,
        "hooks": hooks,
        "tokens": (!tokens.is_null()).then(|| json!({
            "date": tokens["day"]["date"], "zone": tokens["day"]["zone"],
            "startMs": tokens["day"]["startMs"], "endMs": tokens["day"]["endMs"],
            "dayDescription": format!("桌面统计日期 {}，时区 {}", text::string(&tokens["day"]["date"]), text::string(&tokens["day"]["zone"])),
            "dateAccessibility": format!("统计日期 {}，{}", text::string(&tokens["day"]["date"]), text::string(&tokens["day"]["zone"])),
            "total": amount,
        })),
        "tokenFallback": if token_error { "不可用" } else { "未同步" },
        "tokenAccessibilityLabel": format!("总 Token，{}", amount.as_ref().map_or(if token_error { "不可用" } else { "未同步" }, |value| text::string(&value["text"]))),
        "tokenState": if !tokens.is_null() { "available" } else if token_error { "unavailable" } else { "pending" },
        "tokenMessage": if token_error { "Token 暂不可用" } else { "Token 尚未同步" },
        "widgetToken": widget_token,
        "quota": (!providers.is_empty()).then(|| json!({
            "observedAtUnix": snapshot["providerQuota"]["observedAtUnix"], "providers": providers, "description": quota_description,
        })),
        "agents": agent_rows(snapshot, enabled.as_deref()),
        "agentsEmptyText": if !tokens.is_null() { "无分项记录" } else if token_error { "暂不可用" } else { "尚未同步" },
    })
}

fn enabled(snapshot: &Value) -> Option<Vec<String>> {
    snapshot["enabledAgents"].as_array().map(|items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    })
}

fn behavior_label(behavior: Option<&str>) -> &'static str {
    match behavior {
        Some("idle") => "空闲",
        Some("running") => "运行",
        Some("asking") => "询问",
        _ => "出错",
    }
}

fn amount(value: &Value) -> Value {
    let text = formatting::amount_label(value);
    let numeric = formatting::has_numeric_value(value);
    let quality = formatting::amount_quality(value);
    json!({
        "text": text, "compact": formatting::token_compact(&text),
        "compactWide": formatting::token_compact_wide(&text),
        "exact": numeric.then(|| formatting::token_exact(&text)),
        "quality": quality,
        "accessibilityLabel": text::qualified(&format!("总 Token，{text}"), &quality),
    })
}

fn provider(value: &Value) -> Value {
    let name = quota::quota_label(value["provider"].as_str().unwrap_or_default());
    let plan = quota::plan_label(value["plan"].as_str());
    let windows: Vec<_> = quota::quota_windows(value)
        .into_iter()
        .map(|window| {
            let used = window["usedTenths"].as_i64().unwrap_or(0);
            let kind = window["kind"].as_str().unwrap_or("fiveHour");
            let label = quota::kind_label(kind);
            let percent = quota::percent_text(used);
            json!({
                "kind": kind, "label": label, "percent": percent,
                "accessibilityLabel": format!("{label}额度剩余，{percent}"),
                "level": match quota::quota_level(used) { "red" => 0, "yellow" => 1, _ => 2 },
                "remainingTenths": 1_000 - used,
            })
        })
        .collect();
    let display_name = plan
        .as_ref()
        .map_or(name.to_owned(), |plan| format!("{name}，套餐 {plan}"));
    let mut description = vec![display_name.clone()];
    description.extend(windows.iter().map(|window| {
        format!(
            "{}剩余 {}",
            text::string(&window["label"]),
            text::string(&window["percent"])
        )
    }));
    let widget_description = windows
        .iter()
        .map(|window| {
            format!(
                "{display_name} {}额度剩余 {}",
                text::string(&window["label"]),
                text::string(&window["percent"])
            )
        })
        .collect::<Vec<_>>()
        .join("；");
    json!({
        "provider": value["provider"], "name": name, "plan": plan,
        "displayName": display_name,
        "description": description.join("，"), "widgetDescription": widget_description,
        "windows": windows,
    })
}

/// 共享行投影；旧桌面没有启用名单时不新增缺统计的 Cursor 占位，并沿用真实文本。
fn agent_rows(snapshot: &Value, enabled: Option<&[String]>) -> Vec<Value> {
    let tokens = snapshot["tokens"]["agents"].as_array();
    let providers = snapshot["providerQuota"]["providers"].as_array();
    let names = |items: Option<&Vec<Value>>, key: &str| -> Vec<String> {
        items
            .into_iter()
            .flatten()
            .map(|item| item[key].as_str().unwrap_or_default().to_owned())
            .collect()
    };
    agents::project_rows(!snapshot["tokens"].is_null(), &names(tokens, "agent"), &names(providers, "provider"), enabled)
        .into_iter()
        .filter(|row| enabled.is_some() || row.token_index >= 0 || row.quota_index >= 0)
        .map(|row| {
            let agent = tokens.and_then(|items| items.get(row.token_index as usize));
            let provider = providers.and_then(|items| items.get(row.quota_index as usize));
            let source = agent
                .and_then(|value| value["agent"].as_str())
                .or_else(|| provider.and_then(|value| value["provider"].as_str()))
                .unwrap_or("cursor");
            let name = if agent.is_some() || provider.is_none() {
                agent_display_name(source)
            } else {
                quota::quota_label(source).to_owned()
            };
            let raw = agent.map(|value| formatting::amount_label(&value["amount"]));
            let numeric = agent.is_some_and(|value| formatting::has_numeric_value(&value["amount"]));
            let shared = agents::token_text(source, raw.as_deref(), numeric);
            let text = if enabled.is_none() { raw.or(shared) } else { shared }.unwrap_or_else(|| "—".into());
            let quality = agent.map(|value| formatting::amount_quality(&value["amount"])).unwrap_or_default();
            json!({
                "id": format!("{}:{source}", if agent.is_some() { "agent" } else if provider.is_some() { "quota" } else { "default" }),
                "name": name, "plan": quota::plan_label(provider.and_then(|value| value["plan"].as_str())),
                "text": text, "compact": formatting::token_compact(&text),
                "exact": numeric.then(|| formatting::token_exact(&text)),
                "quality": quality,
                "accessibilityLabel": text::qualified(&format!("{name}，{text}"), &quality),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "view_test.rs"]
mod view_test;
