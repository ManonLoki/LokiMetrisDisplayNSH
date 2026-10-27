//! 认证成功之后仍要校验展示快照；坏快照不能让界面进入“已连接”。
use super::err;
use crate::shared::contracts::snapshot::{valid_agent_name, valid_amount, valid_date};
use loki_metris_pairing::assets::AssetDescriptor;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// 报告已连接前校验快照字段、长度与资源描述；返回按摘要索引的资源清单。
pub(super) fn validate_snapshot(v: &Value) -> Result<BTreeMap<String, AssetDescriptor>, String> {
    fn bad() -> String {
        "无效展示快照".into()
    }
    fn optional_text(v: &Value, max: usize) -> bool {
        v.is_null() || v.as_str().is_some_and(|s| s.len() <= max)
    }
    if v["version"] != 1
        || !v["hooksAvailable"].is_boolean()
        || !optional_text(&v["tokenError"], 512)
        || !optional_text(&v["catalogError"], 512)
    {
        return Err(bad());
    }
    let hooks = v["hooks"].as_array().ok_or_else(bad)?;
    let assets = v["assets"].as_array().ok_or_else(bad)?;
    if hooks.len() > 12 || assets.len() > 64 || (v["hooksAvailable"] == false && !hooks.is_empty())
    {
        return Err(bad());
    }
    let mut slots = BTreeSet::new();
    for h in hooks {
        let slot = h["slot"].as_u64().ok_or_else(bad)?;
        if !valid_agent_name(&h["agent"])
            || !(1..=12).contains(&slot)
            || !slots.insert(slot)
            || !h["behavior"]
                .as_str()
                .is_some_and(|x| ["idle", "running", "asking", "error"].contains(&x))
            || h["content"].as_str().is_none_or(|x| x.len() > 512)
            || !h["contentTruncated"].is_boolean()
        {
            return Err(bad());
        }
    }
    let mut descriptors = BTreeMap::new();
    for value in assets {
        let d: AssetDescriptor = serde_json::from_value(value.clone()).map_err(err)?;
        d.validate().map_err(err)?;
        if descriptors.insert(d.hash.clone(), d).is_some() {
            return Err(bad());
        }
    }
    for h in hooks {
        if !h["image"].is_null()
            && h["image"]
                .as_str()
                .is_none_or(|hash| !descriptors.contains_key(hash))
        {
            return Err(bad());
        }
    }
    if !v["tokens"].is_null() {
        if !v["tokenError"].is_null() {
            return Err(bad());
        }
        let t = &v["tokens"];
        let day = &t["day"];
        let date = day["date"].as_str().ok_or_else(bad)?;
        if !valid_date(date)
            || day["zone"].as_str().is_none_or(|s| s.len() > 256)
            || !valid_amount(&t["all"])
        {
            return Err(bad());
        }
        let start = day["startMs"].as_i64().ok_or_else(bad)?;
        let end = day["endMs"].as_i64().ok_or_else(bad)?;
        let observed = t["observedMs"].as_i64().ok_or_else(bad)?;
        if start >= end || observed < start || observed >= end {
            return Err(bad());
        }
        let agents = t["agents"].as_array().ok_or_else(bad)?;
        let mut names = BTreeSet::new();
        if agents.len() > 14 {
            return Err(bad());
        }
        for a in agents {
            if !valid_agent_name(&a["agent"])
                || !valid_amount(&a["amount"])
                || !names.insert(a["agent"].as_str().unwrap().to_owned())
            {
                return Err(bad());
            }
        }
    }
    if !v["enabledAgents"].is_null() {
        let a = v["enabledAgents"].as_array().ok_or_else(bad)?;
        let mut set = BTreeSet::new();
        const KNOWN: [&str; 14] = [
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
        if a.len() > 14
            || a.iter().any(|x| {
                x.as_str()
                    .is_none_or(|s| !KNOWN.contains(&s) || !set.insert(s))
            })
        {
            return Err(bad());
        }
    }
    if !v["providerQuota"].is_null() {
        let quota = &v["providerQuota"];
        if quota["observedAtUnix"].as_i64().is_none_or(|n| n < 0) {
            return Err(bad());
        }
        let p = quota["providers"].as_array().ok_or_else(bad)?;
        if p.len() > 4 {
            return Err(bad());
        }
        let order = ["codex", "claude", "cursor", "grok"];
        let mut previous = None;
        for provider in p {
            let index = order
                .iter()
                .position(|p| Some(*p) == provider["provider"].as_str())
                .ok_or_else(bad)?;
            if previous.is_some_and(|p| p >= index) {
                return Err(bad());
            }
            previous = Some(index);
            if !provider["plan"].is_null()
                && provider["plan"].as_str().is_none_or(|s| {
                    s.is_empty()
                        || s.len() > 24
                        || !s
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b" _-.".contains(&b))
                })
            {
                return Err(bad());
            }
            let mut count = 0;
            for kind in ["fiveHour", "weekly", "monthly"] {
                let w = &provider[kind];
                if w.is_null() {
                    continue;
                }
                count += 1;
                if w["kind"] != kind
                    || w["usedTenths"].as_u64().is_none_or(|n| n > 1000)
                    || (!w["resetAtUnix"].is_null()
                        && w["resetAtUnix"].as_i64().is_none_or(|n| n < 0))
                {
                    return Err(bad());
                }
            }
            if count == 0 {
                return Err(bad());
            }
        }
    }
    Ok(descriptors)
}
