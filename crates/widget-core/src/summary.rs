//! 系统扩展可见的最近同步摘要：只含 Token、额度、启用名单与 Hook 状态，
//! 不含 Hook 正文、图片或本机路径。租约只证明最近一次真实通信。
// 引入整数校验与快照投影
use crate::{contracts::safe_unsigned, snapshot::project_snapshot};
// 引入 JSON
use serde_json::{Value, json};

// 摘要格式版本
pub const VERSION: u64 = 1;
// 租约时长：45 秒
pub const LEASE_MS: i64 = 45_000;
// 摘要最大 64 KiB
pub const MAX_BYTES: usize = 64 * 1024;

/// 已认证的线消息（`type == "snapshot"`）或其中的快照对象都可作为来源。
// 由已认证的线消息（type 为 snapshot）或其中的快照对象构造摘要
pub fn from_message(message: &Value, source_id: &str, received_at_ms: i64) -> Option<Value> {
    // 消息是快照类型
    let snapshot = if message["type"] == "snapshot" {
        // 取其中的快照
        &message["snapshot"]
    // 否则分支
    } else {
        // 否则消息本身就是快照
        message
    };
    // 记录来源、接收时间、租约截止与投影后的快照
    let record = json!({
        // 版本、来源与接收时间
        "version": VERSION, "sourceId": source_id, "receivedAtMs": received_at_ms,
        // 有效期限与快照
        "validUntilMs": received_at_ms.checked_add(LEASE_MS)?, "snapshot": project(snapshot)?,
    });
    // 有效才返回
    validate(&record).then_some(record)
}

/// 线快照与已保存摘要共用同一投影，重复投影结果不变。
// 线快照与已保存的摘要共用同一投影，重复投影结果不变
pub fn project(snapshot: &Value) -> Option<Value> {
    // 复制快照
    let mut input = snapshot.clone();
    // 小组件不用 Hook 正文，补默认值以通过校验
    for hook in input["hooks"].as_array_mut()? {
        // 取 Hook 对象
        let hook = hook.as_object_mut()?;
        // 缺内容则补空串
        hook.entry("content").or_insert(json!(""));
        // 缺截断标记则补 false
        hook.entry("contentTruncated").or_insert(json!(false));
    }
    // 投影出规范快照
    let mut projected = project_snapshot(&input, &Value::Null)?;
    // 只保留代理、槽位和行为，不含正文、图片或本机路径
    for hook in projected["hooks"].as_array_mut()? {
        // 取 Hook 对象
        let hook = hook.as_object_mut()?;
        // 只保留代理、槽位与行为
        hook.retain(|key, _| matches!(key.as_str(), "agent" | "slot" | "behavior"));
    }
    // 日界用于判断“今日”与跨日缓存，原值经过整数校验后保留
    if let Some(day) = snapshot["tokens"]["day"].as_object()
        // 并且取日期对象
        && let Some(target) = projected["tokens"]["day"].as_object_mut()
    {
        // 日界用于判断“今日”与跨日缓存，原值经过整数校验后保留。
        for key in ["startMs", "endMs"] {
            // 取日期字段里的无符号数
            if let Some(value) = day.get(key).and_then(safe_unsigned) {
                // 写入目标
                target.insert(key.into(), json!(value));
            }
        }
    }
    // 返回投影
    Some(projected)
}

// 校验摘要：版本、来源、租约、投影幂等与大小
pub fn validate(record: &Value) -> bool {
    // 接收时间与有效期限都要有
    let (Some(received), Some(until)) = (
        // 接收时间
        record["receivedAtMs"].as_i64(),
        // 有效期限
        record["validUntilMs"].as_i64(),
        // 缺失则否
    ) else {
        // 缺失则无效
        return false;
    };
    // 版本须相符
    record["version"] == VERSION
        // 来源 ID 合法
        && record["sourceId"].as_str().is_some_and(valid_id)
        // 租约时间合法
        && valid_lease(received, until)
        // 快照已是规范投影
        && project(&record["snapshot"]).is_some_and(|snapshot| snapshot == record["snapshot"])
        // 序列化后不超过大小上限
        && serde_json::to_vec(record).is_ok_and(|bytes| bytes.len() <= MAX_BYTES)
}

/// 小写规范 UUID，与连接 owner 的身份规则一致。
// 来源 ID 必须是小写规范 UUID，与连接 owner 的身份规则一致
pub fn valid_id(value: &str) -> bool {
    // 设备标识合法
    loki_metris_pairing::valid_device_id(value)
}

// 租约合法：接收时间在范围内，截止不早于接收加租约
fn valid_lease(received: i64, until: i64) -> bool {
    // 接收时间在范围内且期限不早于租约
    (0..=i64::MAX - LEASE_MS).contains(&received) && until >= received + LEASE_MS
}

/// 最近摘要跨日或断连后仍可展示；未来接收时间与超长租约不可信。
// 摘要跨日或断连后仍可展示；未来的接收时间与超长租约不可信
pub fn is_cached(record: &Value, now_ms: i64) -> bool {
    // 接收时间
    let received = record["receivedAtMs"].as_i64().unwrap_or(-1);
    // 有效期限
    let until = record["validUntilMs"].as_i64().unwrap_or(-1);
    // 记录有效、已接收且在租约内或已过期
    validate(record) && now_ms >= received && (until <= now_ms || until - now_ms <= LEASE_MS)
}

/// 租约内的摘要可让小组件跳过重复请求，但不代表桌面当前在线。
// 租约内的摘要可让小组件跳过重复请求，但不代表桌面当前在线
pub fn is_live(record: &Value, now_ms: i64) -> bool {
    // 缓存可用
    is_cached(record, now_ms)
        // 并且
        && record["validUntilMs"]
            // 有效期限
            .as_i64()
            // 晚于当前时间
            .is_some_and(|until| until > now_ms)
}

/// 只有当前认证连接的心跳续租；旧快照本身不产生在线证明。
// 只有当前认证连接的心跳才续租；旧快照本身不产生在线证明
pub fn renewed(record: &Value, now_ms: i64) -> Option<Value> {
    // 仍在租约内且续租后不溢出
    (is_live(record, now_ms) && now_ms <= i64::MAX - LEASE_MS).then(|| {
        // 复制记录
        let mut next = record.clone();
        // 延长有效期限
        next["validUntilMs"] = json!(now_ms + LEASE_MS);
        // 返回新记录
        next
    })
}

/// 时效判断用的身份：来源与真实接收时间。
// 时效判断用的身份：来源与真实接收时间
pub fn stamp(record: &Value) -> Option<(&str, i64)> {
    // 返回元组
    Some((
        // 来源标识
        record["sourceId"].as_str()?,
        // 接收时间
        record["receivedAtMs"].as_i64()?,
    ))
}

// 测试模块
#[cfg(test)]
#[path = "summary_test.rs"]
pub(crate) mod summary_test;
