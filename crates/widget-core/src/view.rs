//! WidgetKit 的展示投影：所有文字、排序、筛选与阈值都在这里决定，
//! SwiftUI 只按字段绘制，日期按本机区域格式化。
use crate::{
    agents::agent_display_name,
    // 代理、格式化与额度
    usage::{agents, formatting, quota},
};
use serde_json::{Value, json};

#[path = "view_text.rs"]
mod text;

/// 系统先按本机区域格式化日期，核心再生成包含日期的完整描述。
// 系统先按本机区域格式化日期，核心再生成包含日期的完整描述
pub fn describe(request: &Value) -> Value {
    text::describe(request)
}

/// 输入为已校验的摘要快照；只输出 WidgetKit 界面与 [`describe`] 实际读取的字段。
// 输入为已校验的摘要快照；只输出 WidgetKit 界面与 describe 实际读取的字段
pub fn project(snapshot: &Value) -> Value {
    let enabled = enabled(snapshot);
    let tokens = &snapshot["tokens"];
    let token_error = !snapshot["tokenError"].is_null();
    // Hook 是否可用
    let available = snapshot["hooksAvailable"] == true;
    let hooks = snapshot["hooks"].as_array().map_or(&[][..], Vec::as_slice);
    // 槽位最小的 Hook 作为首条摘要
    let lead = hooks
        .iter()
        .min_by_key(|hook| hook["slot"].as_u64())
        // 格式化首个 Hook
        .map(|hook| {
            let name = agent_display_name(hook["agent"].as_str().unwrap_or_default());
            // 格式化为“· 名称 行为”
            format!("· {name} {}", behavior_label(hook["behavior"].as_str()))
        });
    // 只保留启用的供应商
    let providers: Vec<_> = snapshot["providerQuota"]["providers"]
        .as_array()
        .into_iter()
        .flatten()
        // 只保留启用的供应商
        .filter(|provider| {
            agents::is_enabled(
                provider["provider"].as_str().unwrap_or_default(),
                enabled.as_deref(),
            )
        })
        // 逐个供应商转换为展示结构
        .map(provider)
        .collect();
    let amount = (!tokens.is_null()).then(|| amount(&tokens["all"]));
    // 无数据时的占位：有错误为“不可用”，否则“未同步”
    let fallback = if token_error {
        "不可用"
    // 否则分支
    } else {
        "未同步"
    };
    json!({
        // Hooks 数量及其文案
        "hookCountText": if available { hooks.len().to_string() } else { "—".into() },
        "hooksText": if available { format!("Hooks {}", hooks.len()) } else { "Hooks 未同步".into() },
        "hooksSummary": if available { format!("上次 Hooks {}", hooks.len()) } else { "Hooks 未同步".into() },
        "hookLead": lead,
        // Token 统计
        "tokens": (!tokens.is_null()).then(|| json!({
            "date": tokens["day"]["date"],
            "dateAccessibility": format!("统计日期 {}，{}", text::string(&tokens["day"]["date"]), text::string(&tokens["day"]["zone"])),
            "total": amount,
        })),
        "tokenFallback": fallback,
        "tokenAccessibilityLabel": format!("总 Token，{}", amount.as_ref().map_or(fallback, |value| text::string(&value["text"]))),
        "quota": (!providers.is_empty()).then(|| json!({
            // 观测时间与供应商列表
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

// Hook 行为的文案
fn behavior_label(behavior: Option<&str>) -> &'static str {
    match behavior {
        Some("idle") => "空闲",
        Some("running") => "运行",
        Some("asking") => "询问",
        _ => "出错",
    }
}

// 一项用量的各种写法：文本、紧凑、精确与无障碍标签
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

// 供应商的展示：名称、套餐与各窗口
fn provider(value: &Value) -> Value {
    let name = quota::quota_label(value["provider"].as_str().unwrap_or_default());
    let plan = quota::plan_label(value["plan"].as_str());
    // 每个窗口：标签、百分比、等级与剩余千分比
    let windows: Vec<_> = quota::quota_windows(value)
        .into_iter()
        .map(|window| {
            let used = window["usedTenths"].as_i64().unwrap_or(0);
            // 窗口种类，默认五小时
            let kind = window["kind"].as_str().unwrap_or("fiveHour");
            let label = quota::kind_label(kind);
            let percent = quota::percent_text(used);
            json!({
                // 窗口种类、标签与百分比
                "kind": kind, "label": label, "percent": percent,
                "accessibilityLabel": format!("{label}额度剩余，{percent}"),
                // 等级编码：红 0、黄 1、绿 2
                "level": match quota::quota_level(used) { "red" => 0, "yellow" => 1, _ => 2 },
                "remainingTenths": 1_000 - used,
            })
        })
        .collect();
    let display_name = plan
        .as_ref()
        .map_or(name.to_owned(), |plan| format!("{name}，套餐 {plan}"));
    // 无障碍描述：名称加各窗口剩余
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
        // 整体描述与窗口列表
        "description": description.join("，"), "windows": windows,
    })
}

/// 共享行投影；旧桌面没有启用名单时不新增缺统计的 Cursor 占位，并沿用真实文本。
// 共享行投影；旧桌面没有启用名单时不新增缺统计的 Cursor 占位，并沿用真实文本
fn agent_rows(snapshot: &Value, enabled: Option<&[String]>) -> Vec<Value> {
    let tokens = snapshot["tokens"]["agents"].as_array();
    // 额度里的供应商列表
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
        // 没有启用名单时，不显示既无数据又无额度的占位行
        .filter(|row| enabled.is_some() || row.token_index >= 0 || row.quota_index >= 0)
        // 逐行转换为展示结构
        .map(|row| {
            // 对应的 Token 项
            let agent = tokens.and_then(|items| items.get(row.token_index as usize));
            let provider = providers.and_then(|items| items.get(row.quota_index as usize));
            // 来源名：优先代理
            let source = agent
                .and_then(|value| value["agent"].as_str())
                // 都没有时是 Cursor 默认行
                .or_else(|| provider.and_then(|value| value["provider"].as_str()))
                // 默认取 cursor
                .unwrap_or("cursor");
            // 有 Token 项或没有额度项时用代理显示名
            let name = if agent.is_some() || provider.is_none() {
                agent_display_name(source)
            // 否则
            } else {
                quota::quota_label(source).to_owned()
            };
            let raw = agent.map(|value| formatting::amount_label(&value["amount"]));
            let numeric = agent.is_some_and(|value| formatting::has_numeric_value(&value["amount"]));
            // 按共享规则得到文本（Cursor 无数值时显示 0）
            let shared = agents::token_text(source, raw.as_deref(), numeric);
            // 没有启用名单时沿用真实文本，缺失显示“—”
            let text = if enabled.is_none() { raw.or(shared) } else { shared }.unwrap_or_else(|| "—".into());
            let quality = agent.map(|value| formatting::amount_quality(&value["amount"])).unwrap_or_default();
            json!({
                // 行标识：类型加来源
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
