//! WidgetKit 的展示投影：所有文字、排序、筛选与阈值都在这里决定，
//! SwiftUI 只按字段绘制，日期按本机区域格式化。
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

/// 输入为已校验的摘要快照；只输出 WidgetKit 界面与 [`describe`] 实际读取的字段。
pub fn project(snapshot: &Value) -> Value {
    let enabled = enabled(snapshot);
    let tokens = &snapshot["tokens"];
    let token_error = !snapshot["tokenError"].is_null();
    let available = snapshot["hooksAvailable"] == true;
    let hooks = snapshot["hooks"].as_array().map_or(&[][..], Vec::as_slice);
    let lead = hooks
        .iter()
        .min_by_key(|hook| hook["slot"].as_u64())
        .map(|hook| {
            let name = agent_display_name(hook["agent"].as_str().unwrap_or_default());
            format!("· {name} {}", behavior_label(hook["behavior"].as_str()))
        });
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
    let amount = (!tokens.is_null()).then(|| amount(&tokens["all"]));
    let fallback = if token_error {
        "不可用"
    } else {
        "未同步"
    };
    json!({
        "hookCountText": if available { hooks.len().to_string() } else { "—".into() },
        "hooksText": if available { format!("Hooks {}", hooks.len()) } else { "Hooks 未同步".into() },
        "hooksSummary": if available { format!("上次 Hooks {}", hooks.len()) } else { "Hooks 未同步".into() },
        "hookLead": lead,
        "tokens": (!tokens.is_null()).then(|| json!({
            "date": tokens["day"]["date"],
            "dateAccessibility": format!("统计日期 {}，{}", text::string(&tokens["day"]["date"]), text::string(&tokens["day"]["zone"])),
            "total": amount,
        })),
        "tokenFallback": fallback,
        "tokenAccessibilityLabel": format!("总 Token，{}", amount.as_ref().map_or(fallback, |value| text::string(&value["text"]))),
        "quota": (!providers.is_empty()).then(|| json!({
            "observedAtUnix": snapshot["providerQuota"]["observedAtUnix"], "providers": providers,
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
        "exact": numeric.then(|| formatting::token_exact(&text)),
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
    let mut description = vec![display_name];
    description.extend(windows.iter().map(|window| {
        format!(
            "{}剩余 {}",
            text::string(&window["label"]),
            text::string(&window["percent"])
        )
    }));
    json!({
        "provider": value["provider"], "name": name, "plan": plan,
        "description": description.join("，"), "windows": windows,
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
                "accessibilityLabel": text::qualified(&format!("{name}，{text}"), &quality),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "view_test.rs"]
mod view_test;
