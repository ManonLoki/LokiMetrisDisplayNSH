//! 公开展示投影的唯一解码入口；只把白名单字段交给 UI，不透传网络读取密钥。
use crate::{
    shared::contracts::snapshot::{
        safe_unsigned, valid_agent_name, valid_amount, valid_date, valid_hash,
    },
    shared::domain::agents::{canonical_agent_key, normalize_comparison_text},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn parse_snapshot(value: &Value, images: &Value) -> Option<Value> {
    project_snapshot(value, images)
}

/// 同时接受原始 wire 快照和旧缓存投影；网络入口另验证资源描述、时间范围和认证。
pub fn project_snapshot(value: &Value, images: &Value) -> Option<Value> {
    if !value.is_object()
        || safe_unsigned(&value["version"]) != Some(1)
        || !value["hooksAvailable"].is_boolean()
        || !optional_text(&value["tokenError"], 512)
        || (!value["tokenError"].is_null() && !value["tokens"].is_null())
    {
        return None;
    }
    let source_hooks = value["hooks"].as_array()?;
    if source_hooks.len() > 12 || (value["hooksAvailable"] == false && !source_hooks.is_empty()) {
        return None;
    }
    let mut enabled = Value::Null;
    if !value["enabledAgents"].is_null() {
        let values = value["enabledAgents"].as_array()?;
        let mut keys = BTreeSet::new();
        if values.len() > 14
            || values.iter().any(|agent| {
                !valid_agent_name(agent)
                    || !keys.insert(canonical_agent_key(agent.as_str().unwrap_or_default()))
            })
        {
            return None;
        }
        enabled = Value::Array(values.clone());
    }
    let mut slots = BTreeSet::new();
    let mut hooks = Vec::with_capacity(source_hooks.len());
    for item in source_hooks {
        let slot = safe_unsigned(&item["slot"])?;
        if !item.is_object()
            || !valid_agent_name(&item["agent"])
            || !(1..=12).contains(&slot)
            || !slots.insert(slot)
            || !item["behavior"]
                .as_str()
                .is_some_and(|behavior| ["idle", "running", "asking", "error"].contains(&behavior))
            || item["content"]
                .as_str()
                .is_none_or(|content| content.len() > 512)
            || !item["contentTruncated"].is_boolean()
            || (!item["image"].is_null()
                && item["image"].as_str().is_none_or(|hash| !valid_hash(hash)))
        {
            return None;
        }
        let hash = item["image"].as_str();
        let source = hash
            .and_then(|hash| images.get(hash))
            .and_then(Value::as_str)
            .filter(|source| trusted_image_source(source));
        hooks.push(json!({
            "agent": item["agent"], "slot": slot, "behavior": item["behavior"],
            "content": item["content"], "contentTruncated": item["contentTruncated"],
            "image": hash, "imageSource": source,
        }));
    }
    let tokens = if value["tokens"].is_null() {
        Value::Null
    } else {
        project_tokens(&value["tokens"])?
    };
    let quota = if value["providerQuota"].is_null() {
        Value::Null
    } else {
        project_quota(&value["providerQuota"])?
    };
    Some(json!({
        "version": 1, "tokens": tokens, "tokenError": value["tokenError"],
        "hooksAvailable": value["hooksAvailable"], "hooks": hooks,
        "providerQuota": quota, "enabledAgents": enabled,
    }))
}

fn optional_text(value: &Value, maximum: usize) -> bool {
    value.is_null() || value.as_str().is_some_and(|text| text.len() <= maximum)
}

fn trusted_image_source(source: &str) -> bool {
    source.starts_with("file:///")
        || source.starts_with("asset:/")
        || ["png", "gif", "jpeg", "bmp", "webp"]
            .iter()
            .any(|mime| source.starts_with(&format!("data:image/{mime};base64,")))
}

fn amount(value: &Value) -> Option<Value> {
    valid_amount(value).then(|| {
        json!({
            "total": value["total"], "availability": value["availability"],
            "completeness": value["completeness"], "confidence": value["confidence"],
        })
    })
}

fn project_tokens(value: &Value) -> Option<Value> {
    let day = &value["day"];
    let date = day["date"].as_str()?;
    let agents = value["agents"].as_array()?;
    if !value.is_object()
        || !day.is_object()
        || !valid_date(date)
        || day["zone"].as_str().is_none_or(|zone| zone.len() > 256)
        || agents.len() > 14
    {
        return None;
    }
    let mut names = BTreeSet::new();
    let mut output = Vec::with_capacity(agents.len());
    for entry in agents {
        if !entry.is_object()
            || !valid_agent_name(&entry["agent"])
            || !names.insert(normalize_comparison_text(entry["agent"].as_str()?))
        {
            return None;
        }
        output.push(json!({"agent": entry["agent"], "amount": amount(&entry["amount"])?}));
    }
    Some(json!({
        "day": {"date": date, "zone": day["zone"]},
        "all": amount(&value["all"])? , "agents": output,
    }))
}

fn project_quota(value: &Value) -> Option<Value> {
    let observed = safe_unsigned(&value["observedAtUnix"])?;
    let providers = value["providers"].as_array()?;
    if !value.is_object() || providers.len() > 4 {
        return None;
    }
    let order = ["codex", "claude", "cursor", "grok"];
    let mut previous = None;
    let mut output = Vec::with_capacity(providers.len());
    for entry in providers {
        let name = entry["provider"].as_str()?;
        let position = order.iter().position(|provider| *provider == name)?;
        if !entry.is_object()
            || previous.is_some_and(|last| last >= position)
            || (!entry["plan"].is_null()
                && entry["plan"].as_str().is_none_or(|plan| {
                    plan.is_empty()
                        || plan.len() > 24
                        || !plan
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || b" _.-".contains(&byte))
                }))
        {
            return None;
        }
        previous = Some(position);
        let mut provider = json!({"provider": name, "plan": entry["plan"]});
        let mut count = 0;
        for kind in ["monthly", "weekly", "fiveHour"] {
            let window = &entry[kind];
            if window.is_null() {
                continue;
            }
            let tenths = safe_unsigned(&window["usedTenths"])?;
            if !window.is_object() || window["kind"] != kind || tenths > 1000 {
                return None;
            }
            provider[kind] = json!({"kind": kind, "usedTenths": tenths});
            count += 1;
        }
        if count == 0 {
            return None;
        }
        output.push(provider);
    }
    Some(json!({"observedAtUnix": observed, "providers": output}))
}

#[cfg(test)]
#[path = "snapshot_test.rs"]
mod snapshot_test;
