//! WidgetKit 的展示投影：所有文字、排序、筛选与阈值都在这里决定，
//! SwiftUI 只按字段绘制，日期按本机区域格式化。
// 引入代理显示名与用量工具
use crate::{
    // 代理显示名
    agents::agent_display_name,
    // 代理、格式化与额度
    usage::{agents, formatting, quota},
};
// 引入 JSON
use serde_json::{Value, json};

// 描述文案的实现
#[path = "view_text.rs"]
mod text;

/// 系统先按本机区域格式化日期，核心再生成包含日期的完整描述。
// 系统先按本机区域格式化日期，核心再生成包含日期的完整描述
pub fn describe(request: &Value) -> Value {
    // 生成文本描述
    text::describe(request)
}

/// 输入为已校验的摘要快照；只输出 WidgetKit 界面与 [`describe`] 实际读取的字段。
// 输入为已校验的摘要快照；只输出 WidgetKit 界面与 describe 实际读取的字段
pub fn project(snapshot: &Value) -> Value {
    // 启用名单
    let enabled = enabled(snapshot);
    // 用量部分
    let tokens = &snapshot["tokens"];
    // 是否有用量错误
    let token_error = !snapshot["tokenError"].is_null();
    // Hook 是否可用
    let available = snapshot["hooksAvailable"] == true;
    // Hook 列表
    let hooks = snapshot["hooks"].as_array().map_or(&[][..], Vec::as_slice);
    // 槽位最小的 Hook 作为首条摘要
    let lead = hooks
        // 遍历
        .iter()
        // 取槽位最小的
        .min_by_key(|hook| hook["slot"].as_u64())
        // 格式化首个 Hook
        .map(|hook| {
            // 代理显示名
            let name = agent_display_name(hook["agent"].as_str().unwrap_or_default());
            // 格式化为“· 名称 行为”
            format!("· {name} {}", behavior_label(hook["behavior"].as_str()))
        });
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
                provider["provider"].as_str().unwrap_or_default(),
                // 启用名单
                enabled.as_deref(),
            )
        })
        // 逐个供应商转换为展示结构
        .map(provider)
        // 收集为列表
        .collect();
    // 总量
    let amount = (!tokens.is_null()).then(|| amount(&tokens["all"]));
    // 无数据时的占位：有错误为“不可用”，否则“未同步”
    let fallback = if token_error {
        // 不可用
        "不可用"
    // 否则分支
    } else {
        // 未同步
        "未同步"
    };
    // 组装用量展示结构
    json!({
        // Hooks 数量及其文案
        "hookCountText": if available { hooks.len().to_string() } else { "—".into() },
        // Hook 文本
        "hooksText": if available { format!("Hooks {}", hooks.len()) } else { "Hooks 未同步".into() },
        // Hook 摘要
        "hooksSummary": if available { format!("上次 Hooks {}", hooks.len()) } else { "Hooks 未同步".into() },
        // 首个 Hook
        "hookLead": lead,
        // Token 统计
        "tokens": (!tokens.is_null()).then(|| json!({
            // 日期
            "date": tokens["day"]["date"],
            // 日期的读屏写法
            "dateAccessibility": format!("统计日期 {}，{}", text::string(&tokens["day"]["date"]), text::string(&tokens["day"]["zone"])),
            // 总量
            "total": amount,
        })),
        // 备用文案
        "tokenFallback": fallback,
        // 用量读屏文案
        "tokenAccessibilityLabel": format!("总 Token，{}", amount.as_ref().map_or(fallback, |value| text::string(&value["text"]))),
        // 额度
        "quota": (!providers.is_empty()).then(|| json!({
            // 观测时间与供应商列表
            "observedAtUnix": snapshot["providerQuota"]["observedAtUnix"], "providers": providers,
        })),
        // 各代理明细行
        "agents": agent_rows(snapshot, enabled.as_deref()),
        // 无明细时的提示
        "agentsEmptyText": if !tokens.is_null() { "无分项记录" } else if token_error { "暂不可用" } else { "尚未同步" },
    })
}

// 读取启用代理列表
fn enabled(snapshot: &Value) -> Option<Vec<String>> {
    // 启用名单的数组
    snapshot["enabledAgents"].as_array().map(|items| {
        // 遍历条目
        items
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

// Hook 行为的文案
fn behavior_label(behavior: Option<&str>) -> &'static str {
    // 按行为取标签
    match behavior {
        // 空闲
        Some("idle") => "空闲",
        // 运行
        Some("running") => "运行",
        // 询问
        Some("asking") => "询问",
        // 其余为出错
        _ => "出错",
    }
}

// 一项用量的各种写法：文本、紧凑、精确与无障碍标签
fn amount(value: &Value) -> Value {
    // 金额文本
    let text = formatting::amount_label(value);
    // 是否为数值
    let numeric = formatting::has_numeric_value(value);
    // 质量说明
    let quality = formatting::amount_quality(value);
    // 组装金额结构
    json!({
        // 文本与紧凑写法
        "text": text, "compact": formatting::token_compact(&text),
        // 精确写法
        "exact": numeric.then(|| formatting::token_exact(&text)),
        // 读屏文案
        "accessibilityLabel": text::qualified(&format!("总 Token，{text}"), &quality),
    })
}

// 供应商的展示：名称、套餐与各窗口
fn provider(value: &Value) -> Value {
    // 供应商显示名
    let name = quota::quota_label(value["provider"].as_str().unwrap_or_default());
    // 套餐名
    let plan = quota::plan_label(value["plan"].as_str());
    // 每个窗口：标签、百分比、等级与剩余千分比
    let windows: Vec<_> = quota::quota_windows(value)
        // 遍历
        .into_iter()
        // 逐个窗口
        .map(|window| {
            // 已用十分比
            let used = window["usedTenths"].as_i64().unwrap_or(0);
            // 窗口种类，默认五小时
            let kind = window["kind"].as_str().unwrap_or("fiveHour");
            // 窗口标签
            let label = quota::kind_label(kind);
            // 剩余百分比文本
            let percent = quota::percent_text(used);
            // 组装窗口结构
            json!({
                // 窗口种类、标签与百分比
                "kind": kind, "label": label, "percent": percent,
                // 无障碍标签
                "accessibilityLabel": format!("{label}额度剩余，{percent}"),
                // 等级编码：红 0、黄 1、绿 2
                "level": match quota::quota_level(used) { "red" => 0, "yellow" => 1, _ => 2 },
                // 剩余十分比
                "remainingTenths": 1_000 - used,
            })
        })
        // 收集为列表
        .collect();
    // 显示名取套餐
    let display_name = plan
        // 取引用
        .as_ref()
        // 有套餐则附上
        .map_or(name.to_owned(), |plan| format!("{name}，套餐 {plan}"));
    // 无障碍描述：名称加各窗口剩余
    let mut description = vec![display_name];
    // 各窗口追加描述
    description.extend(windows.iter().map(|window| {
        // 格式化
        format!(
            // 格式
            "{}剩余 {}",
            // 标签
            text::string(&window["label"]),
            // 百分比
            text::string(&window["percent"])
        )
    }));
    // 供应商展示结构
    json!({
        // 标识、名称与套餐
        "provider": value["provider"], "name": name, "plan": plan,
        // 整体描述与窗口列表
        "description": description.join("，"), "windows": windows,
    })
}

/// 共享行投影；旧桌面没有启用名单时不新增缺统计的 Cursor 占位，并沿用真实文本。
// 共享行投影；旧桌面没有启用名单时不新增缺统计的 Cursor 占位，并沿用真实文本
fn agent_rows(snapshot: &Value, enabled: Option<&[String]>) -> Vec<Value> {
    // 用量里的代理列表
    let tokens = snapshot["tokens"]["agents"].as_array();
    // 额度里的供应商列表
    let providers = snapshot["providerQuota"]["providers"].as_array();
    // 取名称列表
    let names = |items: Option<&Vec<Value>>, key: &str| -> Vec<String> {
        // 遍历条目
        items
            // 遍历
            .into_iter()
            // 展平
            .flatten()
            // 取字段字符串
            .map(|item| item[key].as_str().unwrap_or_default().to_owned())
            // 收集为列表
            .collect()
    };
    // 计算投影行
    agents::project_rows(!snapshot["tokens"].is_null(), &names(tokens, "agent"), &names(providers, "provider"), enabled)
        // 遍历行的来源
        .into_iter()
        // 没有启用名单时，不显示既无数据又无额度的占位行
        .filter(|row| enabled.is_some() || row.token_index >= 0 || row.quota_index >= 0)
        // 逐行转换为展示结构
        .map(|row| {
            // 对应的 Token 项
            let agent = tokens.and_then(|items| items.get(row.token_index as usize));
            // 对应的额度项
            let provider = providers.and_then(|items| items.get(row.quota_index as usize));
            // 来源名：优先代理
            let source = agent
                // 其次供应商
                .and_then(|value| value["agent"].as_str())
                // 都没有时是 Cursor 默认行
                .or_else(|| provider.and_then(|value| value["provider"].as_str()))
                // 默认取 cursor
                .unwrap_or("cursor");
            // 有 Token 项或没有额度项时用代理显示名
            let name = if agent.is_some() || provider.is_none() {
                // 代理显示名
                agent_display_name(source)
            // 否则
            } else {
                // 用供应商显示名
                quota::quota_label(source).to_owned()
            };
            // 原始用量文本
            let raw = agent.map(|value| formatting::amount_label(&value["amount"]));
            // 是否为数值
            let numeric = agent.is_some_and(|value| formatting::has_numeric_value(&value["amount"]));
            // 按共享规则得到文本（Cursor 无数值时显示 0）
            let shared = agents::token_text(source, raw.as_deref(), numeric);
            // 没有启用名单时沿用真实文本，缺失显示“—”
            let text = if enabled.is_none() { raw.or(shared) } else { shared }.unwrap_or_else(|| "—".into());
            // 质量说明
            let quality = agent.map(|value| formatting::amount_quality(&value["amount"])).unwrap_or_default();
            // 这一行的展示结构
            json!({
                // 行标识：类型加来源
                "id": format!("{}:{source}", if agent.is_some() { "agent" } else if provider.is_some() { "quota" } else { "default" }),
                // 名称与套餐
                "name": name, "plan": quota::plan_label(provider.and_then(|value| value["plan"].as_str())),
                // 文本与紧凑写法
                "text": text, "compact": formatting::token_compact(&text),
                // 精确写法
                "exact": numeric.then(|| formatting::token_exact(&text)),
                // 无障碍标签
                "accessibilityLabel": text::qualified(&format!("{name}，{text}"), &quality),
            })
        })
        // 收集所有行
        .collect()
}

// 测试模块
#[cfg(test)]
#[path = "view_test.rs"]
mod view_test;
