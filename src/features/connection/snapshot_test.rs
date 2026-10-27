use super::*;

fn snapshot() -> Value {
    json!({
        "version": 1, "tokens": null, "tokenError": null, "hooksAvailable": true,
        "hooks": [], "providerQuota": null, "enabledAgents": null,
    })
}

fn amount(total: &str) -> Value {
    json!({"total": total, "availability": "available", "completeness": "complete", "confidence": "exact"})
}

#[test]
fn full_unsigned_tokens_keep_precision_and_overflow_is_rejected() {
    let mut value = snapshot();
    value["tokens"] = json!({"day": {"date": "2026-10-26", "zone": "Asia/Shanghai"},
        "all": amount("18446744073709551615"), "agents": []});
    let output = project_snapshot(&value, &Value::Null).unwrap();
    assert_eq!(output["tokens"]["all"]["total"], "18446744073709551615");
    value["tokens"]["all"]["total"] = json!("18446744073709551616");
    assert!(project_snapshot(&value, &Value::Null).is_none());
}

#[test]
fn public_projection_strips_network_secrets_at_every_level() {
    let mut value = snapshot();
    let hash = "a".repeat(64);
    value["widgetEndpoint"] = json!({"readKey": "secret"});
    value["tokens"] = json!({"day": {"date": "2026-10-26", "zone": "UTC", "private": "secret"},
        "all": amount("42"), "agents": []});
    value["tokens"]["all"]["readKey"] = json!("secret");
    value["hooks"] = json!([{"agent": "codex", "slot": 1, "behavior": "idle", "content": "ok",
        "contentTruncated": false, "image": hash, "readKey": "secret"}]);
    let output = project_snapshot(
        &value,
        &json!({hash.clone(): "file:///private/cache/image"}),
    )
    .unwrap();
    assert_eq!(
        output["hooks"][0]["imageSource"],
        "file:///private/cache/image"
    );
    assert!(!output.to_string().contains("secret"));
    let remote =
        project_snapshot(&value, &json!({hash: "https://untrusted.example/image"})).unwrap();
    assert!(remote["hooks"][0]["imageSource"].is_null());
}

#[test]
fn malformed_lists_quotas_and_duplicate_unicode_identities_are_rejected() {
    let mut value = snapshot();
    value["hooks"] = Value::Null;
    assert!(project_snapshot(&value, &Value::Null).is_none());
    value = snapshot();
    value["enabledAgents"] = json!(["Café", "Cafe\u{301}"]);
    assert!(project_snapshot(&value, &Value::Null).is_none());
    value = snapshot();
    value["providerQuota"] = json!({"observedAtUnix": 1, "providers": [
        {"provider": "codex", "monthly": {"kind": "monthly", "usedTenths": 1001}}
    ]});
    assert!(project_snapshot(&value, &Value::Null).is_none());
    value["providerQuota"]["providers"][0]["monthly"]["usedTenths"] = json!(999);
    assert!(project_snapshot(&value, &Value::Null).is_some());
}
