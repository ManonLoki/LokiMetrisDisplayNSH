// 引入被测模块
use super::*;

// 最小的合法快照
fn snapshot() -> Value {
    json!({
        "version": 1, "tokens": null, "tokenError": null, "hooksAvailable": true,
        "hooks": [], "providerQuota": null, "enabledAgents": null,
    })
}

// 构造一项完整的可用用量
fn amount(total: &str) -> Value {
    json!({"total": total, "availability": "available", "completeness": "complete", "confidence": "exact"})
}

// 完整的无符号整数保持精度，溢出被拒绝
#[test]
fn full_unsigned_tokens_keep_precision_and_overflow_is_rejected() {
    let mut value = snapshot();
    // u64 最大值
    value["tokens"] = json!({"day": {"date": "2026-10-26", "zone": "Asia/Shanghai"},
        "all": amount("18446744073709551615"), "agents": []});
    let output = project_snapshot(&value, &Value::Null).unwrap();
    // 原样保留
    assert_eq!(output["tokens"]["all"]["total"], "18446744073709551615");
    // 超出 u64 一位
    value["tokens"]["all"]["total"] = json!("18446744073709551616");
    // 被拒绝
    assert!(project_snapshot(&value, &Value::Null).is_none());
}

// 公开投影在每一层都剥离网络秘密
#[test]
fn public_projection_strips_network_secrets_at_every_level() {
    let mut value = snapshot();
    let hash = "a".repeat(64);
    // 在多处塞入含 secret 的字段
    value["widgetEndpoint"] = json!({"readKey": "secret"});
    value["tokens"] = json!({"day": {"date": "2026-10-26", "zone": "UTC", "private": "secret"},
        "all": amount("42"), "agents": []});
    value["tokens"]["all"]["readKey"] = json!("secret");
    value["hooks"] = json!([{"agent": "codex", "slot": 1, "behavior": "idle", "content": "ok",
        "contentTruncated": false, "image": hash, "readKey": "secret"}]);
    // 本地缓存地址被保留
    let output = project_snapshot(
        &value,
        &json!({hash.clone(): "file:///private/cache/image"}),
    )
    .unwrap();
    assert_eq!(
        output["hooks"][0]["imageSource"],
        "file:///private/cache/image"
    );
    // 输出中不应出现任何秘密
    assert!(!output.to_string().contains("secret"));
    // 远程地址不受信任，imageSource 为空
    let remote =
        project_snapshot(&value, &json!({hash: "https://untrusted.example/image"})).unwrap();
    assert!(remote["hooks"][0]["imageSource"].is_null());
}

// 格式错误的列表、额度与归一后重复的名称被拒绝
#[test]
fn malformed_lists_quotas_and_duplicate_unicode_identities_are_rejected() {
    let mut value = snapshot();
    // hooks 不是数组
    value["hooks"] = Value::Null;
    assert!(project_snapshot(&value, &Value::Null).is_none());
    value = snapshot();
    // 同一字符的两种 Unicode 写法算重复
    value["enabledAgents"] = json!(["Café", "Cafe\u{301}"]);
    assert!(project_snapshot(&value, &Value::Null).is_none());
    value = snapshot();
    // 已用千分比超过 1000
    value["providerQuota"] = json!({"observedAtUnix": 1, "providers": [
        {"provider": "codex", "monthly": {"kind": "monthly", "usedTenths": 1001}}
    ]});
    assert!(project_snapshot(&value, &Value::Null).is_none());
    // 修正后通过
    value["providerQuota"]["providers"][0]["monthly"]["usedTenths"] = json!(999);
    assert!(project_snapshot(&value, &Value::Null).is_some());
}
