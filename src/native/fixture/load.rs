//! 读取并校验测试夹具、已配对记录和缓存快照：任何一处不合法就整体忽略。
use super::scenario::Event;
use crate::features::connection::{identity::valid_device_id, snapshot::project_snapshot};
use serde_json::{Map, Value, json};

// 安全整数：非负且不超过 2^53-1
fn safe_int(value: &Value) -> Option<u64> {
    crate::shared::contracts::snapshot::safe_unsigned(value)
}

// 读取事件夹具；任何一处不合法就整体忽略
pub(super) fn read_fixture(raw: &str) -> (Vec<(String, String)>, Vec<Event>) {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return (Vec::new(), Vec::new());
    };
    if !value.is_object() {
        return (Vec::new(), Vec::new());
    }
    let mut discovered = Vec::new();
    if let Some(items) = value.get("discovered").filter(|items| !items.is_null()) {
        let Some(items) = items.as_array() else {
            return (Vec::new(), Vec::new());
        };
        for item in items {
            match (item["id"].as_str(), item["name"].as_str()) {
                (Some(id), Some(name)) if valid_device_id(id) => {
                    discovered.push((id.to_owned(), name.to_owned()))
                }
                _ => return (Vec::new(), Vec::new()),
            }
        }
    }
    let mut events = Vec::new();
    if let Some(items) = value.get("events").filter(|items| !items.is_null()) {
        let Some(items) = items.as_array() else {
            return (Vec::new(), Vec::new());
        };
        for item in items {
            let kind = item["kind"].as_str().unwrap_or("");
            let Some(at_ms) = item["atMs"].as_f64().filter(|at| at.is_finite()) else {
                return (Vec::new(), Vec::new());
            };
            if !["handshake", "snapshot", "transport-fail"].contains(&kind) {
                return (Vec::new(), Vec::new());
            }
            events.push(Event {
                at_ms: at_ms.max(0.0) as u64,
                kind: kind.to_owned(),
                snapshot: item["snapshot"].clone(),
                received_at_ms: item["receivedAtMs"].as_u64().unwrap_or(0),
                images: if item["images"].is_object() {
                    item["images"].clone()
                } else {
                    json!({})
                },
            });
        }
    }
    (discovered, events)
}

// 读取已配对记录，丢弃不合法的项
pub(super) fn read_paired(raw: Option<&str>) -> Map<String, Value> {
    let mut result = Map::new();
    let Some(Value::Object(map)) = raw.and_then(|raw| serde_json::from_str::<Value>(raw).ok())
    else {
        return result;
    };
    for (id, item) in map {
        if valid_device_id(&id)
            && item["clientId"].is_string()
            && safe_int(&item["savedAtMs"]).is_some()
        {
            result.insert(
                id,
                json!({"clientId": item["clientId"], "savedAtMs": item["savedAtMs"]}),
            );
        }
    }
    result
}

// 读取缓存快照
pub(super) fn read_cache(raw: Option<&str>) -> Option<(String, u64, Value)> {
    let value: Value = serde_json::from_str(raw?).ok()?;
    let source = value["sourceId"]
        .as_str()
        .filter(|id| valid_device_id(id))?;
    let received = safe_int(&value["receivedAtMs"])?;
    let snapshot = project_snapshot(&value["snapshot"], &json!({}))?;
    Some((source.to_owned(), received, snapshot))
}
