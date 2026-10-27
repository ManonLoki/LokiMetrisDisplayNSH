use super::*;
use crate::summary::summary_test::{SOURCE, message};
use loki_metris_pairing::widget::{derive_key, open_request, seal_response};

const DEVICE: &str = "d2d2d2d2-3333-4ddd-8ddd-dddddddddddd";

fn granted() -> ([u8; 32], Value) {
    let key = derive_key(&[7; 32], SOURCE, DEVICE).unwrap();
    (
        key,
        target(
            SOURCE,
            DEVICE,
            "192.168.1.20",
            51_303,
            &STANDARD.encode(key),
        )
        .unwrap(),
    )
}

/// 客户端与桌面真实服务端实现往返：请求可被打开，响应绑定本次 nonce。
#[test]
fn request_and_response_roundtrip_with_desktop_protocol() {
    let (key, target) = granted();
    let request = request(&target, 5_000).unwrap();
    assert_eq!(
        request["url"],
        "http://192.168.1.20:51303/api/widget/v1/snapshot"
    );
    let body: Value = serde_json::from_str(request["body"].as_str().unwrap()).unwrap();
    assert_eq!(body["clientId"], SOURCE);
    let nonce = open_request(
        &key,
        body["nonce"].as_str().unwrap(),
        body["ciphertext"].as_str().unwrap(),
    )
    .unwrap();
    let plaintext = serde_json::to_vec(&message()).unwrap();
    let (response_nonce, ciphertext) = seal_response(&key, &nonce, &[9; 12], &plaintext).unwrap();
    let envelope = json!({"nonce": response_nonce, "ciphertext": ciphertext}).to_string();
    let record = response(
        &target,
        &request,
        200,
        Some("application/json; charset=utf-8"),
        envelope.as_bytes(),
    )
    .unwrap();
    assert_eq!(record["receivedAtMs"], 5_000);
    assert_eq!(
        record["snapshot"]["hooks"][0],
        json!({"agent": "Codex", "slot": 2, "behavior": "running"})
    );

    // 换一次请求 nonce 后旧响应不可重放；状态码与类型也必须匹配。
    let other = super::request(&target, 5_000).unwrap();
    assert!(
        response(
            &target,
            &other,
            200,
            Some("application/json"),
            envelope.as_bytes()
        )
        .is_err()
    );
    assert!(
        response(
            &target,
            &request,
            500,
            Some("application/json"),
            envelope.as_bytes()
        )
        .is_err()
    );
    assert!(
        response(
            &target,
            &request,
            200,
            Some("text/html"),
            envelope.as_bytes()
        )
        .is_err()
    );
}

#[test]
fn hosts_are_limited_to_private_lan_addresses() {
    for host in [
        "10.0.0.1",
        "172.16.4.2",
        "192.168.1.20",
        "169.254.3.4",
        "127.0.0.1",
        "fe80::1%en0",
        "fd00::2",
        "mac-mini.local",
        "studio.local.",
    ] {
        assert!(valid_host(host), "{host}");
    }
    for host in [
        "8.8.8.8",
        "192.168.01.2",
        "example.com",
        "Mac.local",
        "2001:db8::1",
        "fe80::1%EN0",
        "-a.local",
        "",
    ] {
        assert!(!valid_host(host), "{host}");
    }
    let (_, target) = granted();
    let mut scoped = target.clone();
    scoped["host"] = json!("fe80::1%en0");
    assert_eq!(
        url(&scoped).unwrap(),
        "http://[fe80::1%25en0]:51303/api/widget/v1/snapshot"
    );
}

#[test]
fn targets_come_only_from_declared_capability() {
    let (key, _) = granted();
    let mut message = message();
    message["widgetEndpoint"] = json!({"version": 1, "port": 51_303, "key": STANDARD.encode(key)});
    assert!(target_from_message(&message, SOURCE, DEVICE, "192.168.1.20").is_none());
    message["capabilities"] = json!(["widgetHttpV1"]);
    let first = target_from_message(&message, SOURCE, DEVICE, "192.168.1.20").unwrap();
    let second = target_from_message(&message, SOURCE, DEVICE, "192.168.1.20").unwrap();
    assert_ne!(first["generation"], second["generation"]);
    assert!(same_connection(&first, &second));
    assert!(target_from_message(&message, SOURCE, DEVICE, "8.8.8.8").is_none());
}
