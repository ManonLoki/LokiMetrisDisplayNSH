//! 公开展示投影的唯一解码入口；只把白名单字段交给 UI，不透传网络读取密钥。
// 引入校验辅助与代理名规范化
use crate::{
    // 快照契约的校验函数
    shared::contracts::snapshot::{
        // 无符号数、代理名、金额、日期与哈希校验
        safe_unsigned,
        // 代理名合法
        valid_agent_name,
        // 金额合法
        valid_amount,
        // 日期合法
        valid_date,
        // 哈希合法
        valid_hash,
    },
    // 规范代理键与比较文本规范化
    shared::domain::{agents::canonical_agent_key, text::normalize_comparison_text},
};
// 引入 JSON 值与构造宏
use serde_json::{Value, json};
// 引入有序集合用于去重
use std::collections::BTreeSet;

/// 同时接受原始 wire 快照和旧缓存投影；网络入口另验证资源描述、时间范围和认证。
// 快照投影入口：校验并只保留白名单字段
pub fn project_snapshot(value: &Value, images: &Value) -> Option<Value> {
    // 版本必须为 1，各字段类型正确，且错误与统计不能同时存在
    if !value.is_object()
        // 或者版本不是 1
        || safe_unsigned(&value["version"]) != Some(1)
        // 或者 Hook 可用标记不是布尔
        || !value["hooksAvailable"].is_boolean()
        // 或者用量错误文本不合法
        || !optional_text(&value["tokenError"], 512)
        // 或者同时有错误和用量
        || (!value["tokenError"].is_null() && !value["tokens"].is_null())
    {
        // 无效则返回 None
        return None;
    }
    // 原始 Hook 列表
    let source_hooks = value["hooks"].as_array()?;
    // 最多 12 条；Hooks 不可用时必须为空
    if source_hooks.len() > 12 || (value["hooksAvailable"] == false && !source_hooks.is_empty()) {
        // 无效则返回 None
        return None;
    }
    // 启用代理列表，默认不过滤
    let mut enabled = Value::Null;
    // 有列表则校验：最多 14 项、名称合法、规范键不重复
    if !value["enabledAgents"].is_null() {
        // 启用名单必须是数组
        let values = value["enabledAgents"].as_array()?;
        // 已见代理键集合
        let mut keys = BTreeSet::new();
        // 数量不得超过 14
        if values.len() > 14
            // 或者名单里有不合格的代理
            || values.iter().any(|agent| {
                // 代理名必须合法
                !valid_agent_name(agent)
                    // 或者规范键重复
                    || !keys.insert(canonical_agent_key(agent.as_str().unwrap_or_default()))
            })
        {
            // 不合法则整体无效
            return None;
        }
        // 保留启用名单
        enabled = Value::Array(values.clone());
    }
    // 已出现的槽位，不允许重复
    let mut slots = BTreeSet::new();
    // 投影后的 Hook
    let mut hooks = Vec::with_capacity(source_hooks.len());
    // 逐条校验
    for item in source_hooks {
        // 槽位序号
        let slot = safe_unsigned(&item["slot"])?;
        // 槽位 1 到 12 且唯一；行为、正文长度、截断标记与图片哈希都要合法
        if !item.is_object()
            // 或者代理名不合法
            || !valid_agent_name(&item["agent"])
            // 或者槽位不在 1 到 12
            || !(1..=12).contains(&slot)
            // 或者槽位重复
            || !slots.insert(slot)
            // 或者行为不是四种之一
            || !item["behavior"]
                // 取字符串
                .as_str()
                // 检查取值
                .is_some_and(|behavior| ["idle", "running", "asking", "error"].contains(&behavior))
            // 或者内容
            || item["content"]
                // 内容必须是字符串
                .as_str()
                // 且不超过 512 字节
                .is_none_or(|content| content.len() > 512)
            // 截断标记必须是布尔
            || !item["contentTruncated"].is_boolean()
            // 图片若存在
            || (!item["image"].is_null()
                // 哈希必须合法
                && item["image"].as_str().is_none_or(|hash| !valid_hash(hash)))
        {
            // 任一不合法则整体无效
            return None;
        }
        // 图片哈希
        let hash = item["image"].as_str();
        // 本地图片地址必须来自受信任的来源前缀
        let source = hash
            // 按哈希取图片来源
            .and_then(|hash| images.get(hash))
            // 取字符串
            .and_then(Value::as_str)
            // 来源须可信
            .filter(|source| trusted_image_source(source));
        // 加入一个 Hook
        hooks.push(json!({
            // 代理、槽位与行为
            "agent": item["agent"], "slot": slot, "behavior": item["behavior"],
            // 内容与截断标记
            "content": item["content"], "contentTruncated": item["contentTruncated"],
            // 图片哈希与来源
            "image": hash, "imageSource": source,
        }));
    }
    // Token 统计：为空保持空，否则逐项投影
    let tokens = if value["tokens"].is_null() {
        // 没有用量则为 null
        Value::Null
    // 否则分支
    } else {
        // 投影用量
        project_tokens(&value["tokens"])?
    };
    // 额度：同上
    let quota = if value["providerQuota"].is_null() {
        // 没有额度则为 null
        Value::Null
    // 否则分支
    } else {
        // 投影额度
        project_quota(&value["providerQuota"])?
    };
    // 组装快照
    Some(json!({
        // 版本、用量与错误
        "version": 1, "tokens": tokens, "tokenError": value["tokenError"],
        // Hook 可用标记与 Hook 列表
        "hooksAvailable": value["hooksAvailable"], "hooks": hooks,
        // 额度与启用名单
        "providerQuota": quota, "enabledAgents": enabled,
    }))
}

// 可空文本，且不超过上限
fn optional_text(value: &Value, maximum: usize) -> bool {
    // 为空或不超过最大长度的字符串
    value.is_null() || value.as_str().is_some_and(|text| text.len() <= maximum)
}

// 图片地址只接受本地文件、应用资源或内嵌图片
fn trusted_image_source(source: &str) -> bool {
    // 图片来源须是 file:/// 地址
    source.starts_with("file:///")
        // 或者以 asset:/ 开头
        || source.starts_with("asset:/")
        // 或者是受支持类型的 data URL
        || ["png", "gif", "jpeg", "bmp", "webp"]
            // 遍历
            .iter()
            // 任一类型匹配
            .any(|mime| source.starts_with(&format!("data:image/{mime};base64,")))
}

// 校验并复制一项用量
fn amount(value: &Value) -> Option<Value> {
    // 金额合法才投影
    valid_amount(value).then(|| {
        // 组装金额对象
        json!({
            // 总量与可用性等字段
            "total": value["total"], "availability": value["availability"],
            // 完整性与置信度
            "completeness": value["completeness"], "confidence": value["confidence"],
        })
    })
}

// 投影 Token 统计
fn project_tokens(value: &Value) -> Option<Value> {
    // 统计日
    let day = &value["day"];
    // 日期
    let date = day["date"].as_str()?;
    // 各代理用量
    let agents = value["agents"].as_array()?;
    // 日期格式、时区长度与代理数量都有限制
    if !value.is_object()
        // 或者日期不是对象
        || !day.is_object()
        // 或者日期不合法
        || !valid_date(date)
        // 或者时区过长
        || day["zone"].as_str().is_none_or(|zone| zone.len() > 256)
        // 或者代理超过 14 个
        || agents.len() > 14
    {
        // 无效则返回 None
        return None;
    }
    // 规范化后的代理名，不允许重复
    let mut names = BTreeSet::new();
    // 输出列表
    let mut output = Vec::with_capacity(agents.len());
    // 逐个校验
    for entry in agents {
        // 条目须是对象
        if !entry.is_object()
            // 或者代理名不合法
            || !valid_agent_name(&entry["agent"])
            // 或者规范化名称重复
            || !names.insert(normalize_comparison_text(entry["agent"].as_str()?))
        {
            // 无效则返回 None
            return None;
        }
        // 加入一个代理条目
        output.push(json!({"agent": entry["agent"], "amount": amount(&entry["amount"])?}));
    }
    // 组装用量结构
    Some(json!({
        // 日期与时区
        "day": {"date": date, "zone": day["zone"]},
        // 总量与各代理
        "all": amount(&value["all"])? , "agents": output,
    }))
}

// 投影供应商额度
fn project_quota(value: &Value) -> Option<Value> {
    // 观测时间
    let observed = safe_unsigned(&value["observedAtUnix"])?;
    // 供应商列表
    let providers = value["providers"].as_array()?;
    // 最多四个供应商
    if !value.is_object() || providers.len() > 4 {
        // 无效则返回 None
        return None;
    }
    // 供应商必须按固定顺序出现
    let order = ["codex", "claude", "cursor", "grok"];
    // 上一个供应商的位置
    let mut previous = None;
    // 输出列表
    let mut output = Vec::with_capacity(providers.len());
    // 逐个校验
    for entry in providers {
        // 供应商名
        let name = entry["provider"].as_str()?;
        // 不在白名单内则整体无效
        let position = order.iter().position(|provider| *provider == name)?;
        // 顺序递增；套餐名只允许短小的字母数字与 _ . - 空格
        if !entry.is_object()
            // 或者位置不递增
            || previous.is_some_and(|last| last >= position)
            // 或者套餐不合法
            || (!entry["plan"].is_null()
                // 套餐字符串
                && entry["plan"].as_str().is_none_or(|plan| {
                    // 套餐为空
                    plan.is_empty()
                        // 或者套餐过长
                        || plan.len() > 24
                        // 或者套餐含不允许的字符
                        || !plan
                            // 逐字节
                            .bytes()
                            // 只允许字母数字及空格、下划线、点、连字符
                            .all(|byte| byte.is_ascii_alphanumeric() || b" _.-".contains(&byte))
                }))
        {
            // 不合法则整体无效
            return None;
        }
        // 记下本次位置
        previous = Some(position);
        // 供应商基础字段
        let mut provider = json!({"provider": name, "plan": entry["plan"]});
        // 已有窗口数
        let mut count = 0;
        // 按月、周、五小时检查窗口
        for kind in ["monthly", "weekly", "fiveHour"] {
            // 额度窗口
            let window = &entry[kind];
            // 缺失的窗口跳过，不补零
            if window.is_null() {
                // 没有该窗口则跳过
                continue;
            }
            // 已用千分比不超过 1000
            let tenths = safe_unsigned(&window["usedTenths"])?;
            // 窗口须合法且用量不超过 1000
            if !window.is_object() || window["kind"] != kind || tenths > 1000 {
                // 无效则返回 None
                return None;
            }
            // 写入窗口
            provider[kind] = json!({"kind": kind, "usedTenths": tenths});
            // 窗口计数加一
            count += 1;
        }
        // 至少要有一个窗口
        if count == 0 {
            // 无效则返回 None
            return None;
        }
        // 加入该供应商
        output.push(provider);
    }
    // 返回额度快照
    Some(json!({"observedAtUnix": observed, "providers": output}))
}

// 测试模块
#[cfg(test)]
#[path = "snapshot_test.rs"]
mod snapshot_test;
