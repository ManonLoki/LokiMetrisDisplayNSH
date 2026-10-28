//! 认证成功之后仍要校验展示快照；坏快照不能让界面进入“已连接”。
//! 字段规则只在 `project_snapshot` 维护一份；这里只补原始网络帧独有的约束。
// 引入统一错误
use super::err;
// 引入快照投影
use crate::features::connection::snapshot::project_snapshot;
// 引入资源描述
use loki_metris_pairing::assets::AssetDescriptor;
// 引入 JSON
use serde_json::Value;
// 引入集合
use std::collections::{BTreeMap, BTreeSet};

/// 协议中 `enabledAgents` 的规范拼写；投影只按归一化键去重，网络帧还要求精确键名。
// 协议中 enabledAgents 的规范拼写；投影只按归一化键去重，网络帧还要求精确键名
const WIRE_AGENTS: [&str; 14] = [
    // Codex 智能体
    "codex",
    // Claude Code 智能体
    "claudeCode",
    // Cursor 智能体
    "cursor",
    // OpenCode 智能体
    "openCode",
    // WorkBuddy 智能体
    "workBuddy",
    // Hermes 智能体
    "hermes",
    // OpenClaw 智能体
    "openClaw",
    // CodeBuddy 的写法
    // CodeBuddy 的写法
    "codeBuddy",
    // Qwen Code 的写法
    // Qwen Code 的写法
    "qwenCode",
    // Kimi Code 的写法
    // Kimi Code 的写法
    "kimiCode",
    // Qoder 的写法
    // Qoder 的写法
    "qoder",
    // Gemini CLI 的写法
    // Gemini CLI 的写法
    "geminiCli",
    // GitHub Copilot 的写法
    // GitHub Copilot 的写法
    "gitHubCopilot",
    // Grok 的写法
    // Grok 的写法
    "grok",
];

/// 报告已连接前校验快照字段、长度与资源描述；返回按摘要索引的资源清单。
// 报告“已连接”之前校验快照字段、长度与资源描述；返回按摘要索引的资源清单
pub(super) fn validate_snapshot(v: &Value) -> Result<BTreeMap<String, AssetDescriptor>, String> {
    // 统一的错误文案
    fn bad() -> String {
        // 无效展示快照
        "无效展示快照".into()
    }
    // 字段规则由投影维护；错误信息长度另有限制
    if project_snapshot(v, &Value::Null).is_none()
        // 目录错误非空时，须是合法字符串
        || !v["catalogError"].is_null() && v["catalogError"].as_str().is_none_or(|s| s.len() > 512)
    {
        // 无效
        return Err(bad());
    }
    // 资源列表必须存在
    let assets = v["assets"].as_array().ok_or_else(bad)?;
    // 最多 64 个
    if assets.len() > 64 {
        // 无效
        return Err(bad());
    }
    // 资源描述表
    let mut descriptors = BTreeMap::new();
    // 逐个解析并校验描述，摘要不能重复
    for value in assets {
        // 解析资源描述
        let d: AssetDescriptor = serde_json::from_value(value.clone()).map_err(err)?;
        // 校验资源描述
        d.validate().map_err(err)?;
        // 哈希重复
        if descriptors.insert(d.hash.clone(), d).is_some() {
            // 无效
            return Err(bad());
        }
    }
    // 每条 Hook 引用的图片必须在资源列表里
    for h in v["hooks"].as_array().into_iter().flatten() {
        // Hook 的图片引用
        if h["image"]
            // 取字符串
            .as_str()
            // 哈希不在资源表里
            .is_some_and(|hash| !descriptors.contains_key(hash))
        {
            // 引用不在资源表里则无效
            return Err(bad());
        }
    }
    // Token 统计的时间范围：开始早于结束，观测时刻落在其中
    if !v["tokens"].is_null() {
        // 用量部分
        let t = &v["tokens"];
        // 当天起始毫秒
        let start = t["day"]["startMs"].as_i64().ok_or_else(bad)?;
        // 当天结束毫秒
        let end = t["day"]["endMs"].as_i64().ok_or_else(bad)?;
        // 观察时间
        let observed = t["observedMs"].as_i64().ok_or_else(bad)?;
        // 起止顺序或观察时间不合理
        if start >= end || observed < start || observed >= end {
            // 无效
            return Err(bad());
        }
    }
    // 启用列表必须使用精确的协议键名且不重复
    if !v["enabledAgents"].is_null() {
        // 已见代理集合
        let mut seen = BTreeSet::new();
        // 启用名单是否全部合法
        let exact = v["enabledAgents"]
            // 转为数组
            .as_array()
            // 转为迭代器
            .into_iter()
            // 展平
            .flatten()
            // 逐项检查
            .all(|x| {
                // 取字符串
                x.as_str()
                    // 须在白名单且不重复
                    .is_some_and(|s| WIRE_AGENTS.contains(&s) && seen.insert(s))
            });
        // 不合格
        if !exact {
            // 返回错误
            return Err(bad());
        }
    }
    // 额度的时间戳必须为非负
    if !v["providerQuota"].is_null() {
        // 额度部分
        let quota = &v["providerQuota"];
        // 观察时间不能为负
        if quota["observedAtUnix"].as_i64().is_none_or(|n| n < 0) {
            // 无效
            return Err(bad());
        }
        // 逐个窗口检查重置时间
        for provider in quota["providers"].as_array().into_iter().flatten() {
            // 逐种窗口
            for kind in ["fiveHour", "weekly", "monthly"] {
                // 重置时间
                let reset = &provider[kind]["resetAtUnix"];
                // 存在但为负数
                if !reset.is_null() && reset.as_i64().is_none_or(|n| n < 0) {
                    // 无效
                    return Err(bad());
                }
            }
        }
    }
    // 返回描述列表
    Ok(descriptors)
}
