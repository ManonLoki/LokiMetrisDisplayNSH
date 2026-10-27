//! 系统扩展可见的最近同步摘要：只含 Token、额度、启用名单与 Hook 状态，
//! 不含 Hook 正文、图片或本机路径。租约只证明最近一次真实通信。
use crate::{contracts::safe_unsigned, snapshot::project_snapshot};
use serde_json::{Value, json};

pub const VERSION: u64 = 1;
pub const LEASE_MS: i64 = 45_000;
pub const MAX_BYTES: usize = 64 * 1024;

/// 已认证的线消息（`type == "snapshot"`）或其中的快照对象都可作为来源。
pub fn from_message(message: &Value, source_id: &str, received_at_ms: i64) -> Option<Value> {
    let snapshot = if message["type"] == "snapshot" {
        &message["snapshot"]
    } else {
        message
    };
    let record = json!({
        "version": VERSION, "sourceId": source_id, "receivedAtMs": received_at_ms,
        "validUntilMs": received_at_ms.checked_add(LEASE_MS)?, "snapshot": project(snapshot)?,
    });
    validate(&record).then_some(record)
}

/// 线快照与已保存摘要共用同一投影，重复投影结果不变。
pub fn project(snapshot: &Value) -> Option<Value> {
    let mut input = snapshot.clone();
    for hook in input["hooks"].as_array_mut()? {
        let hook = hook.as_object_mut()?;
        hook.entry("content").or_insert(json!(""));
        hook.entry("contentTruncated").or_insert(json!(false));
    }
    let mut projected = project_snapshot(&input, &Value::Null)?;
    for hook in projected["hooks"].as_array_mut()? {
        let hook = hook.as_object_mut()?;
        hook.retain(|key, _| matches!(key.as_str(), "agent" | "slot" | "behavior"));
    }
    if let Some(day) = snapshot["tokens"]["day"].as_object()
        && let Some(target) = projected["tokens"]["day"].as_object_mut()
    {
        // 日界用于判断“今日”与跨日缓存，原值经过整数校验后保留。
        for key in ["startMs", "endMs"] {
            if let Some(value) = day.get(key).and_then(safe_unsigned) {
                target.insert(key.into(), json!(value));
            }
        }
    }
    Some(projected)
}

pub fn validate(record: &Value) -> bool {
    let (Some(received), Some(until)) = (
        record["receivedAtMs"].as_i64(),
        record["validUntilMs"].as_i64(),
    ) else {
        return false;
    };
    record["version"] == VERSION
        && record["sourceId"].as_str().is_some_and(valid_id)
        && valid_lease(received, until)
        && project(&record["snapshot"]).is_some_and(|snapshot| snapshot == record["snapshot"])
        && serde_json::to_vec(record).is_ok_and(|bytes| bytes.len() <= MAX_BYTES)
}

/// 小写规范 UUID，与连接 owner 的身份规则一致。
pub fn valid_id(value: &str) -> bool {
    loki_metris_pairing::valid_device_id(value)
}

fn valid_lease(received: i64, until: i64) -> bool {
    (0..=i64::MAX - LEASE_MS).contains(&received) && until >= received + LEASE_MS
}

/// 最近摘要跨日或断连后仍可展示；未来接收时间与超长租约不可信。
pub fn is_cached(record: &Value, now_ms: i64) -> bool {
    let received = record["receivedAtMs"].as_i64().unwrap_or(-1);
    let until = record["validUntilMs"].as_i64().unwrap_or(-1);
    validate(record) && now_ms >= received && (until <= now_ms || until - now_ms <= LEASE_MS)
}

/// 租约内的摘要可让小组件跳过重复请求，但不代表桌面当前在线。
pub fn is_live(record: &Value, now_ms: i64) -> bool {
    is_cached(record, now_ms)
        && record["validUntilMs"]
            .as_i64()
            .is_some_and(|until| until > now_ms)
}

/// 只有当前认证连接的心跳续租；旧快照本身不产生在线证明。
pub fn renewed(record: &Value, now_ms: i64) -> Option<Value> {
    (is_live(record, now_ms) && now_ms <= i64::MAX - LEASE_MS).then(|| {
        let mut next = record.clone();
        next["validUntilMs"] = json!(now_ms + LEASE_MS);
        next
    })
}

/// 时效判断用的身份：来源与真实接收时间。
pub fn stamp(record: &Value) -> Option<(&str, i64)> {
    Some((
        record["sourceId"].as_str()?,
        record["receivedAtMs"].as_i64()?,
    ))
}

#[cfg(test)]
#[path = "summary_test.rs"]
pub(crate) mod summary_test;
