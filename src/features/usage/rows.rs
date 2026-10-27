use super::{agents, formatting, quota};
use crate::shared::domain::agents::agent_display_name;
use serde_json::{Value, json};

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
    let tokens = snapshot["tokens"]["agents"].as_array();
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
        let text = agents::token_text(source, raw.as_deref(), numeric);
        let exact = text
            .as_deref()
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

/// One bridge call gives the card all business facts. Layout measurement and
/// device-local time formatting remain in the component.
pub fn card_projection(snapshot: &Value) -> Value {
    let tokens = &snapshot["tokens"];
    let all = &tokens["all"];
    let has_tokens = !tokens.is_null();
    let has_exact = !all["total"].is_null();
    let enabled = enabled_agents(snapshot);
    let providers: Vec<_> = snapshot["providerQuota"]["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|provider| {
            agents::is_enabled(
                provider["provider"].as_str().unwrap_or(""),
                enabled.as_deref(),
            )
        })
        .map(quota::provider_projection)
        .collect();
    json!({
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
