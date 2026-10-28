// 引入被测摘要模块
use super::*;

// 测试用来源 ID
pub(crate) const SOURCE: &str = "c1c1c1c1-2222-4ccc-8ccc-cccccccccccc";

// 一条完整的测试快照消息：Token、Hook（含私密正文）与额度
pub(crate) fn message() -> Value {
    json!({"type": "snapshot", "sourceId": SOURCE, "snapshot": {
        "version": 1, "tokenError": null, "hooksAvailable": true, "enabledAgents": null,
        "tokens": {"day": {"date": "2026-10-27", "zone": "Asia/Shanghai", "startMs": 1, "endMs": 86_400_001},
            "all": {"total": "18446744073709551615", "availability": "available", "completeness": "partial", "confidence": "derived"},
            "agents": [{"agent": "Codex", "amount": {"total": "1500", "availability": "available", "completeness": "complete", "confidence": "exact"}}]},
        "hooks": [{"agent": "Codex", "slot": 2, "behavior": "running", "content": "private text",
            "contentTruncated": false, "image": "a".repeat(64)}],
        // 额度：Codex 的五小时窗口
        "providerQuota": {"observedAtUnix": 10, "providers": [{"provider": "codex", "plan": "plus",
            // 已用 25%
            "fiveHour": {"kind": "fiveHour", "usedTenths": 250}}]},
    }})
}

#[test]
// 摘要去掉 Hook 的私密字段，但保留日界
fn summary_drops_private_hook_fields_and_keeps_day_bounds() {
    let record = from_message(&message(), SOURCE, 1_000).unwrap();
    let hook = &record["snapshot"]["hooks"][0];
    // Hook 只剩代理、槽位与行为，正文和图片都被去掉
    assert_eq!(
        hook,
        &json!({"agent": "Codex", "slot": 2, "behavior": "running"})
    );
    // 日界被保留
    assert_eq!(record["snapshot"]["tokens"]["day"]["endMs"], 86_400_001);
    // 租约截止 = 接收时间 + 45 秒
    assert_eq!(record["validUntilMs"], 1_000 + LEASE_MS);
    assert!(validate(&record));
    // 已保存摘要再次投影结果不变，缓存可直接校验。
    // 已保存摘要再投影结果不变，缓存可直接校验
    assert_eq!(project(&record["snapshot"]).unwrap(), record["snapshot"]);
}

#[test]
// 非法来源与消息被拒绝
fn invalid_sources_and_messages_are_rejected() {
    // 来源不是 UUID：拒绝
    assert!(from_message(&message(), "not-a-uuid", 1).is_none());
    // 版本不对的快照：拒绝
    assert!(
        from_message(
            &json!({"type": "snapshot", "snapshot": {"version": 2}}),
            SOURCE,
            1
        )
        .is_none()
    );
    // 摘要被塞回私密字段：校验不通过
    let mut record = from_message(&message(), SOURCE, 1).unwrap();
    record["snapshot"]["hooks"][0]["content"] = json!("leak");
    assert!(!validate(&record));
}

#[test]
// 租约把在线、缓存与不可信记录区分开
fn lease_separates_live_cached_and_untrusted_records() {
    let record = from_message(&message(), SOURCE, 1_000).unwrap();
    // 刚收到：既在线又可作缓存
    assert!(is_live(&record, 1_000) && is_cached(&record, 1_000));
    // 租约到期后不再在线，但仍可作缓存
    assert!(!is_live(&record, 1_000 + LEASE_MS) && is_cached(&record, 10_000_000));
    // 接收时间在未来：不可信
    assert!(!is_cached(&record, 999), "future receive time");
    // 租约内续租：截止时间顺延
    let renewed = renewed(&record, 30_000).unwrap();
    assert_eq!(renewed["validUntilMs"], 30_000 + LEASE_MS);
    // 租约过期后不能续租
    assert!(
        super::renewed(&record, 1_000 + LEASE_MS).is_none(),
        "expired lease cannot renew"
    );
    // 伪造超长租约：不可信
    let mut forged = record.clone();
    forged["validUntilMs"] = json!(10_000_000);
    assert!(!is_cached(&forged, 2_000));
}
