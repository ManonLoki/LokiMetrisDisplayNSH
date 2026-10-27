use serde_json::{Value, json};

pub fn quota_label(provider: &str) -> &'static str {
    match provider {
        "codex" => "Codex",
        "claude" => "Claude",
        "cursor" => "Cursor",
        _ => "Grok",
    }
}

pub fn plan_label(plan: Option<&str>) -> Option<String> {
    let plan = plan.filter(|value| !value.is_empty())?;
    let mut chars = plan.chars();
    Some(format!(
        "{}{}",
        chars.next()?.to_uppercase(),
        chars.as_str()
    ))
}

/// Only project windows supplied by the service; missing monthly/weekly data
/// must not appear as an invented zero or unlimited balance.
pub fn quota_windows(provider: &Value) -> Vec<&Value> {
    ["monthly", "weekly", "fiveHour"]
        .into_iter()
        .filter_map(|key| provider.get(key).filter(|window| !window.is_null()))
        .collect()
}

pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        "monthly" => "每月",
        "weekly" => "每周",
        _ => "5h",
    }
}

pub fn percent_text(used_tenths: i64) -> String {
    let remaining = 1_000 - used_tenths;
    if remaining % 10 == 0 {
        format!("{}%", remaining / 10)
    } else {
        format!("{}.{}%", remaining.div_euclid(10), remaining % 10)
    }
}

pub fn quota_level(used_tenths: i64) -> &'static str {
    match 1_000 - used_tenths {
        ..=200 => "red",
        201..=500 => "yellow",
        _ => "green",
    }
}

pub fn display_windows(provider: &Value) -> Vec<Value> {
    quota_windows(provider)
        .into_iter()
        .map(|window| {
            let used = window["usedTenths"].as_i64().unwrap_or(0);
            json!({
                "kindLabel": kind_label(window["kind"].as_str().unwrap_or("fiveHour")),
                "percent": percent_text(used),
                "level": quota_level(used),
                "remainingTenths": 1_000 - used
            })
        })
        .collect()
}

pub fn provider_projection(provider: &Value) -> Value {
    let name = provider["provider"].as_str().unwrap_or("");
    json!({
        "provider": name,
        "name": quota_label(name),
        "plan": plan_label(provider["plan"].as_str()),
        "windows": display_windows(provider)
    })
}

#[cfg(test)]
#[path = "quota_test.rs"]
mod quota_test;
