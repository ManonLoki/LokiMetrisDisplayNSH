//! 认证成功之后仍要校验展示快照；坏快照不能让界面进入“已连接”。
//! 字段规则只在 `project_snapshot` 维护一份；这里只补原始网络帧独有的约束。
use super::err;
use crate::features::connection::snapshot::project_snapshot;
use loki_metris_pairing::assets::AssetDescriptor;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// 协议中 `enabledAgents` 的规范拼写；投影只按归一化键去重，网络帧还要求精确键名。
const WIRE_AGENTS: [&str; 14] = [
    "codex",
    "claudeCode",
    "cursor",
    "openCode",
    "workBuddy",
    "hermes",
    "openClaw",
    "codeBuddy",
    "qwenCode",
    "kimiCode",
    "qoder",
    "geminiCli",
    "gitHubCopilot",
    "grok",
];

/// 报告已连接前校验快照字段、长度与资源描述；返回按摘要索引的资源清单。
pub(super) fn validate_snapshot(v: &Value) -> Result<BTreeMap<String, AssetDescriptor>, String> {
    fn bad() -> String {
        "无效展示快照".into()
    }
    if project_snapshot(v, &Value::Null).is_none()
        || !v["catalogError"].is_null() && v["catalogError"].as_str().is_none_or(|s| s.len() > 512)
    {
        return Err(bad());
    }
    let assets = v["assets"].as_array().ok_or_else(bad)?;
    if assets.len() > 64 {
        return Err(bad());
    }
    let mut descriptors = BTreeMap::new();
    for value in assets {
        let d: AssetDescriptor = serde_json::from_value(value.clone()).map_err(err)?;
        d.validate().map_err(err)?;
        if descriptors.insert(d.hash.clone(), d).is_some() {
            return Err(bad());
        }
    }
    for h in v["hooks"].as_array().into_iter().flatten() {
        if h["image"]
            .as_str()
            .is_some_and(|hash| !descriptors.contains_key(hash))
        {
            return Err(bad());
        }
    }
    if !v["tokens"].is_null() {
        let t = &v["tokens"];
        let start = t["day"]["startMs"].as_i64().ok_or_else(bad)?;
        let end = t["day"]["endMs"].as_i64().ok_or_else(bad)?;
        let observed = t["observedMs"].as_i64().ok_or_else(bad)?;
        if start >= end || observed < start || observed >= end {
            return Err(bad());
        }
    }
    if !v["enabledAgents"].is_null() {
        let mut seen = BTreeSet::new();
        let exact = v["enabledAgents"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|x| {
                x.as_str()
                    .is_some_and(|s| WIRE_AGENTS.contains(&s) && seen.insert(s))
            });
        if !exact {
            return Err(bad());
        }
    }
    if !v["providerQuota"].is_null() {
        let quota = &v["providerQuota"];
        if quota["observedAtUnix"].as_i64().is_none_or(|n| n < 0) {
            return Err(bad());
        }
        for provider in quota["providers"].as_array().into_iter().flatten() {
            for kind in ["fiveHour", "weekly", "monthly"] {
                let reset = &provider[kind]["resetAtUnix"];
                if !reset.is_null() && reset.as_i64().is_none_or(|n| n < 0) {
                    return Err(bad());
                }
            }
        }
    }
    Ok(descriptors)
}
