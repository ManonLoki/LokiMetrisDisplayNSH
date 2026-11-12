use serde_json::{Value, json};

pub fn quota_label(provider: &str) -> &'static str {
    match provider {
        // Codex 智能体
        "codex" => "Codex",
        // Claude 智能体
        "claude" => "Claude",
        // Cursor 智能体
        "cursor" => "Cursor",
        _ => "Grok",
    }
}

// 套餐名首字母大写；空值返回 None
pub fn plan_label(plan: Option<&str>) -> Option<String> {
    // 空套餐返回 None
    let plan = plan.filter(|value| !value.is_empty())?;
    let mut chars = plan.chars();
    Some(format!(
        "{}{}",
        chars.next()?.to_uppercase(),
        chars.as_str()
    ))
}

/// 只投影服务提供的窗口;缺失的月度/周度数据
/// 不得显示成凭空捏造的零值或无限余额。
// 只投影服务端给出的窗口，缺失的不捏造为 0 或无限
pub fn quota_windows(provider: &Value) -> Vec<&Value> {
    // 固定顺序：月、周、五小时
    ["monthly", "weekly", "fiveHour"]
        .into_iter()
        // 取出存在且非 null 的窗口
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

// 剩余额度的百分比文本，整十时不带小数
pub fn percent_text(used_tenths: i64) -> String {
    let remaining = 1_000 - used_tenths;
    // 整十：显示整数百分比
    if remaining % 10 == 0 {
        // 剩余十分比换算为百分比
        format!("{}%", remaining / 10)
    // 否则分支
    } else {
        // 否则保留一位小数
        format!("{}.{}%", remaining.div_euclid(10), remaining % 10)
    }
}

// 按剩余千分比分级：不超过 20% 红色，不超过 50% 黄色，其余绿色
pub fn quota_level(used_tenths: i64) -> &'static str {
    match 1_000 - used_tenths {
        // 剩余不超过 20%
        ..=200 => "red",
        // 剩余 20% 到 50%
        201..=500 => "yellow",
        _ => "green",
    }
}

// 把窗口转换为展示结构
pub fn display_windows(provider: &Value) -> Vec<Value> {
    // 取该供应商的额度窗口
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
