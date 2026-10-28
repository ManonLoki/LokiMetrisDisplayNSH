// 引入同模块的代理、格式化与额度工具
use super::{agents, formatting, quota};
// 引入代理显示名
use crate::shared::domain::agents::agent_display_name;
// 引入 JSON 值与构造宏
use serde_json::{Value, json};

// 读取启用代理列表，缺失表示不过滤
fn enabled_agents(snapshot: &Value) -> Option<Vec<String>> {
    // 启用名单
    snapshot["enabledAgents"].as_array().map(|agents| {
        // 遍历
        agents
            // 遍历
            .iter()
            // 取字符串
            .filter_map(Value::as_str)
            // 转为自有字符串
            .map(str::to_owned)
            // 收集为列表
            .collect()
    })
}

// 生成明细行
pub fn display_rows(snapshot: &Value) -> Vec<Value> {
    // Token 与额度的原始列表
    let tokens = snapshot["tokens"]["agents"].as_array();
    // 额度里的供应商列表
    let providers = snapshot["providerQuota"]["providers"].as_array();
    // 各行的代理名
    let token_names: Vec<_> = tokens
        // 转为迭代器
        .into_iter()
        // 展平
        .flatten()
        // 取代理名
        .map(|agent| agent["agent"].as_str().unwrap_or("").to_owned())
        // 收集为列表
        .collect();
    // 各供应商名
    let quota_names: Vec<_> = providers
        // 转为迭代器
        .into_iter()
        // 展平
        .flatten()
        // 取供应商名
        .map(|provider| provider["provider"].as_str().unwrap_or("").to_owned())
        // 收集为列表
        .collect();
    // 启用列表
    let enabled = enabled_agents(snapshot);
    // 合并出行的来源
    agents::project_rows(
        // 是否有用量
        !snapshot["tokens"].is_null(),
        // 代理名列表
        &token_names,
        // 供应商名列表
        &quota_names,
        // 启用名单
        enabled.as_deref(),
    )
    // 转为迭代器
    .into_iter()
    // 把每行的来源转换成展示数据
    .map(|row| {
        // 对应的 Token 项
        let agent = tokens.and_then(|items| items.get(row.token_index as usize));
        // 对应的额度项
        let provider = providers.and_then(|items| items.get(row.quota_index as usize));
        // 来源名：优先代理，其次供应商，都没有则是 Cursor 默认行
        let source = agent
            // 取代理名
            .and_then(|value| value["agent"].as_str())
            // 其次取供应商名
            .or_else(|| provider.and_then(|value| value["provider"].as_str()))
            // 默认 cursor
            .unwrap_or("cursor");
        // 有 Token 项用代理显示名，仅有额度时用供应商名
        let name = if agent.is_some() || provider.is_none() {
            // 代理显示名
            agent_display_name(source)
        // 否则分支
        } else {
            // 供应商显示名
            quota::quota_label(source).to_owned()
        };
        // 原始用量文本
        let raw = agent.map(|value| formatting::amount_label(&value["amount"]));
        // 是否为数值
        let numeric = agent.is_some_and(|value| formatting::has_numeric_value(&value["amount"]));
        // Cursor 无数值时显示 0
        let text = agents::token_text(source, raw.as_deref(), numeric);
        // 精确写法只在数值或 0 时提供
        let exact = text
            // 取引用
            .as_deref()
            // 数值或 0 才保留
            .filter(|value| numeric || *value == "0")
            // 精确写法
            .map(formatting::token_exact);
        // 组装行
        json!({
            // 名称
            "name": name,
            // 文本
            "tokenText": text,
            // 紧凑写法
            "compact": text.as_deref().map(formatting::token_compact),
            // 精确值
            "exact": exact,
            // 套餐名
            "plan": quota::plan_label(provider.and_then(|value| value["plan"].as_str())),
            // 额度窗口
            "windows": provider.map(quota::display_windows).unwrap_or_default()
        })
    })
    // 收集为列表
    .collect()
}

/// 一次桥接调用就给卡片提供全部业务事实。布局测量和
/// 设备本地时间的格式化仍留在组件中。
// 一次桥接调用给出卡片所需的全部业务事实；布局测量与本地时间格式化仍在组件内
pub fn card_projection(snapshot: &Value) -> Value {
    // 用量部分
    let tokens = &snapshot["tokens"];
    // 总量
    let all = &tokens["all"];
    // 是否有 Token 数据
    let has_tokens = !tokens.is_null();
    // 是否有精确值
    let has_exact = !all["total"].is_null();
    // 启用名单
    let enabled = enabled_agents(snapshot);
    // 只保留启用的供应商
    let providers: Vec<_> = snapshot["providerQuota"]["providers"]
        // 转为数组
        .as_array()
        // 转为迭代器
        .into_iter()
        // 展平
        .flatten()
        // 只保留启用的供应商
        .filter(|provider| {
            // 代理是否启用
            agents::is_enabled(
                // 供应商名
                provider["provider"].as_str().unwrap_or(""),
                // 启用名单
                enabled.as_deref(),
            )
        })
        // 转换为供应商投影
        .map(quota::provider_projection)
        // 收集为列表
        .collect();
    // 汇总结构
    json!({
        // 是否有 Token 数据
        "hasTokens": has_tokens,
        // 是否有精确值
        "hasExact": has_exact,
        // 日期
        "date": tokens["day"]["date"],
        // 时区
        "zone": tokens["day"]["zone"],
        // 总量的紧凑写法
        "total": if has_tokens { formatting::token_compact(&formatting::amount_label(all)) } else { String::new() },
        // 总量的精确写法
        "exact": has_exact.then(|| formatting::token_exact(&formatting::amount_label(all))),
        // 总量的读屏写法
        "amountAccessibility": has_tokens.then(|| formatting::amount_accessibility(all)),
        // 缺失时的提示文案
        "missingText": if snapshot["tokenError"].is_null() { "等待同步" } else { "暂不可用" },
        // 供应商列表
        "providers": providers,
        // 明细行
        "rows": if snapshot.is_null() { Vec::new() } else { display_rows(snapshot) },
        // 额度观测时间
        "quotaObservedAtUnix": snapshot["providerQuota"]["observedAtUnix"].as_u64().unwrap_or(0)
    })
}

#[cfg(test)]
#[path = "rows_test.rs"]
// 行的测试模块
mod rows_test;
