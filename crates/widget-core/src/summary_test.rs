use super::*;

pub(crate) const SOURCE: &str = "c1c1c1c1-2222-4ccc-8ccc-cccccccccccc";

pub(crate) fn message() -> Value {
    json!({"type": "snapshot", "sourceId": SOURCE, "snapshot": {
        "version": 1, "tokenError": null, "hooksAvailable": true, "enabledAgents": null,
        "tokens": {"day": {"date": "2026-10-27", "zone": "Asia/Shanghai", "startMs": 1, "endMs": 86_400_001},
            "all": {"total": "18446744073709551615", "availability": "available", "completeness": "partial", "confidence": "derived"},
            "agents": [{"agent": "Codex", "amount": {"total": "1500", "availability": "available", "completeness": "complete", "confidence": "exact"}}]},
        "hooks": [{"agent": "Codex", "slot": 2, "behavior": "running", "content": "private text",
            "contentTruncated": false, "image": "a".repeat(64)}],
        "providerQuota": {"observedAtUnix": 10, "providers": [{"provider": "codex", "plan": "plus",
            "fiveHour": {"kind": "fiveHour", "usedTenths": 250}}]},
    }})
}

#[test]
fn summary_drops_private_hook_fields_and_keeps_day_bounds() {
    let record = from_message(&message(), SOURCE, 1_000).unwrap();
    let hook = &record["snapshot"]["hooks"][0];
    assert_eq!(
        hook,
        &json!({"agent": "Codex", "slot": 2, "behavior": "running"})
    );
    assert_eq!(record["snapshot"]["tokens"]["day"]["endMs"], 86_400_001);
    assert_eq!(record["validUntilMs"], 1_000 + LEASE_MS);
    assert!(validate(&record));
    // 已保存摘要再次投影结果不变，缓存可直接校验。
    assert_eq!(project(&record["snapshot"]).unwrap(), record["snapshot"]);
}

#[test]
fn invalid_sources_and_messages_are_rejected() {
    assert!(from_message(&message(), "not-a-uuid", 1).is_none());
    assert!(
        from_message(
            &json!({"type": "snapshot", "snapshot": {"version": 2}}),
            SOURCE,
            1
        )
        .is_none()
    );
    let mut record = from_message(&message(), SOURCE, 1).unwrap();
    record["snapshot"]["hooks"][0]["content"] = json!("leak");
    assert!(!validate(&record));
}

#[test]
fn lease_separates_live_cached_and_untrusted_records() {
    let record = from_message(&message(), SOURCE, 1_000).unwrap();
    assert!(is_live(&record, 1_000) && is_cached(&record, 1_000));
    assert!(!is_live(&record, 1_000 + LEASE_MS) && is_cached(&record, 10_000_000));
    assert!(!is_cached(&record, 999), "future receive time");
    let renewed = renewed(&record, 30_000).unwrap();
    assert_eq!(renewed["validUntilMs"], 30_000 + LEASE_MS);
    assert!(
        super::renewed(&record, 1_000 + LEASE_MS).is_none(),
        "expired lease cannot renew"
    );
    let mut forged = record.clone();
    forged["validUntilMs"] = json!(10_000_000);
    assert!(!is_cached(&forged, 2_000));
}
