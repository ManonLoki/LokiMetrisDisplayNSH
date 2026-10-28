// 引入 JSON 值与构造宏
use serde_json::{Value, json};

// 供应商的显示名
pub fn quota_label(provider: &str) -> &'static str {
    // 按供应商取显示名
    match provider {
        // Codex 智能体
        "codex" => "Codex",
        // Claude 智能体
        "claude" => "Claude",
        // Cursor 智能体
        "cursor" => "Cursor",
        // 其余为 Grok
        _ => "Grok",
    }
}

// 套餐名首字母大写；空值返回 None
pub fn plan_label(plan: Option<&str>) -> Option<String> {
    // 空套餐返回 None
    let plan = plan.filter(|value| !value.is_empty())?;
    // 套餐字符迭代器
    let mut chars = plan.chars();
    // 首字母大写
    Some(format!(
        // 格式
        "{}{}",
        // 首字符大写
        chars.next()?.to_uppercase(),
        // 其余字符保持不变
        chars.as_str()
    ))
}

/// 只投影服务提供的窗口;缺失的月度/周度数据
/// 不得显示成凭空捏造的零值或无限余额。
// 只投影服务端给出的窗口，缺失的不捏造为 0 或无限
pub fn quota_windows(provider: &Value) -> Vec<&Value> {
    // 固定顺序：月、周、五小时
    ["monthly", "weekly", "fiveHour"]
        // 转为迭代器
        .into_iter()
        // 取出存在且非 null 的窗口
        .filter_map(|key| provider.get(key).filter(|window| !window.is_null()))
        // 收集为列表
        .collect()
}

// 窗口种类的显示名
pub fn kind_label(kind: &str) -> &'static str {
    // 按窗口种类取标签
    match kind {
        // 每月
        "monthly" => "每月",
        // 每周
        "weekly" => "每周",
        // 其余为 5h
        _ => "5h",
    }
}

// 剩余额度的百分比文本，整十时不带小数
pub fn percent_text(used_tenths: i64) -> String {
    // 剩余千分比
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
    // 按剩余十分比分级
    match 1_000 - used_tenths {
        // 剩余不超过 20%
        ..=200 => "red",
        // 剩余 20% 到 50%
        201..=500 => "yellow",
        // 其余
        _ => "green",
    }
}

// 把窗口转换为展示结构
pub fn display_windows(provider: &Value) -> Vec<Value> {
    // 取该供应商的额度窗口
    quota_windows(provider)
        // 转为迭代器
        .into_iter()
        // 逐个窗口转换
        .map(|window| {
            // 已用千分比
            let used = window["usedTenths"].as_i64().unwrap_or(0);
            // 窗口展示结构
            json!({
                // 种类标签
                "kindLabel": kind_label(window["kind"].as_str().unwrap_or("fiveHour")),
                // 剩余百分比
                "percent": percent_text(used),
                // 等级
                "level": quota_level(used),
                // 剩余千分比
                "remainingTenths": 1_000 - used
            })
        })
        // 收集为列表
        .collect()
}

// 供应商的展示结构
pub fn provider_projection(provider: &Value) -> Value {
    // 供应商名
    let name = provider["provider"].as_str().unwrap_or("");
    // 组装供应商投影
    json!({
        // 供应商标识
        "provider": name,
        // 显示名
        "name": quota_label(name),
        // 套餐
        "plan": plan_label(provider["plan"].as_str()),
        // 窗口列表
        "windows": display_windows(provider)
    })
}

// 测试模块
#[cfg(test)]
#[path = "quota_test.rs"]
mod quota_test;
