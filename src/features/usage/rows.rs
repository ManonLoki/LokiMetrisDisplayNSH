use super::{agents, formatting, quota};
use crate::shared::domain::agents::agent_display_name;
use serde_json::{Value, json};

// 读取启用代理列表，缺失表示不过滤
fn enabled_agents(snapshot: &Value) -> Option<Vec<String>> {
    snapshot["enabledAgents"].as_array().map(|agents| {
        agents
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    })
}

pub fn display_rows(snapshot: &Value) -> Vec<Value> {
    // Token 与额度的原始列表
    let tokens = snapshot["tokens"]["agents"].as_array();
    // 额度里的供应商列表
    let providers = snapshot["providerQuota"]["providers"].as_array();
    let token_names: Vec<_> = tokens
        .into_iter()
        .flatten()
        .map(|agent| agent["agent"].as_str().unwrap_or("").to_owned())
        .collect();
    let quota_names: Vec<_> = providers
        .into_iter()
        .flatten()
        .map(|provider| provider["provider"].as_str().unwrap_or("").to_owned())
        .collect();
    let enabled = enabled_agents(snapshot);
    agents::project_rows(
        !snapshot["tokens"].is_null(),
        &token_names,
        &quota_names,
        enabled.as_deref(),
    )
    .into_iter()
    // 把每行的来源转换成展示数据
    .map(|row| {
        // 对应的 Token 项
        let agent = tokens.and_then(|items| items.get(row.token_index as usize));
        let provider = providers.and_then(|items| items.get(row.quota_index as usize));
        // 来源名：优先代理，其次供应商，都没有则是 Cursor 默认行
        let source = agent
            .and_then(|value| value["agent"].as_str())
            .or_else(|| provider.and_then(|value| value["provider"].as_str()))
            // 默认 cursor
            .unwrap_or("cursor");
        // 有 Token 项用代理显示名，仅有额度时用供应商名
        let name = if agent.is_some() || provider.is_none() {
            agent_display_name(source)
        // 否则分支
        } else {
            quota::quota_label(source).to_owned()
        };
        let raw = agent.map(|value| formatting::amount_label(&value["amount"]));
        let numeric = agent.is_some_and(|value| formatting::has_numeric_value(&value["amount"]));
        // Cursor 无数值时显示 0
        let text = agents::token_text(source, raw.as_deref(), numeric);
        // 精确写法只在数值或 0 时提供
        let exact = text
            .as_deref()
            // 数值或 0 才保留
            .filter(|value| numeric || *value == "0")
            .map(formatting::token_exact);
        json!({
            "name": name,
            "tokenText": text,
            "compact": text.as_deref().map(formatting::token_compact),
            "exact": exact,
            "plan": quota::plan_label(provider.and_then(|value| value["plan"].as_str())),
            "windows": provider.map(quota::display_windows).unwrap_or_default()
        })
    })
    .collect()
}

/// 一次桥接调用就给卡片提供全部业务事实。布局测量和
/// 设备本地时间的格式化仍留在组件中。
// 一次桥接调用给出卡片所需的全部业务事实；布局测量与本地时间格式化仍在组件内
pub fn card_projection(snapshot: &Value) -> Value {
    let tokens = &snapshot["tokens"];
    let all = &tokens["all"];
    // 是否有 Token 数据
    let has_tokens = !tokens.is_null();
    let has_exact = !all["total"].is_null();
    let enabled = enabled_agents(snapshot);
    // 只保留启用的供应商
    let providers: Vec<_> = snapshot["providerQuota"]["providers"]
        .as_array()
        .into_iter()
        .flatten()
        // 只保留启用的供应商
        .filter(|provider| {
            agents::is_enabled(
                provider["provider"].as_str().unwrap_or(""),
                enabled.as_deref(),
            )
        })
        .map(quota::provider_projection)
        .collect();
    json!({
        // 是否有 Token 数据
        "hasTokens": has_tokens,
        "hasExact": has_exact,
        "date": tokens["day"]["date"],
        "zone": tokens["day"]["zone"],
        "total": if has_tokens { formatting::token_compact(&formatting::amount_label(all)) } else { String::new() },
        "exact": has_exact.then(|| formatting::token_exact(&formatting::amount_label(all))),
        "amountAccessibility": has_tokens.then(|| formatting::amount_accessibility(all)),
        "missingText": if snapshot["tokenError"].is_null() { "等待同步" } else { "暂不可用" },
        "providers": providers,
        "rows": if snapshot.is_null() { Vec::new() } else { display_rows(snapshot) },
        "quotaObservedAtUnix": snapshot["providerQuota"]["observedAtUnix"].as_u64().unwrap_or(0)
    })
}

#[cfg(test)]
#[path = "rows_test.rs"]
mod rows_test;
