// 引入被测 HTTP 协议
use super::*;
// 复用摘要测试里的消息夹具
use crate::summary::summary_test::{SOURCE, message};
// 引入桌面端的协议实现作为对照
use loki_metris_pairing::widget::{derive_key, open_request, seal_response};

// 测试用的设备 ID
const DEVICE: &str = "d2d2d2d2-3333-4ddd-8ddd-dddddddddddd";

// 模拟桌面授予读取凭据
fn granted() -> ([u8; 32], Value) {
    // 由配对秘密派生读取密钥
    let key = derive_key(&[7; 32], SOURCE, DEVICE).unwrap();
    // 返回密钥与以它构造的读取凭据
    (
        key,
        target(
            SOURCE,
            DEVICE,
            "192.168.1.20",
            51_303,
            &STANDARD.encode(key),
        )
        // 构造凭据，失败则测试失败
        .unwrap(),
    )
}

/// 客户端与桌面真实服务端实现往返：请求可被打开，响应绑定本次 nonce。
#[test]
// 请求与响应能与桌面端协议往返互通
fn request_and_response_roundtrip_with_desktop_protocol() {
    let (key, target) = granted();
    // 生成请求，地址固定为私有主机
    let request = request(&target, 5_000).unwrap();
    assert_eq!(
        request["url"],
        "http://192.168.1.20:51303/api/widget/v1/snapshot"
    );
    // 请求体里的客户端 ID 是来源
    let body: Value = serde_json::from_str(request["body"].as_str().unwrap()).unwrap();
    assert_eq!(body["clientId"], SOURCE);
    // 用桌面端实现打开请求，取出 nonce
    let nonce = open_request(
        &key,
        body["nonce"].as_str().unwrap(),
        body["ciphertext"].as_str().unwrap(),
    )
    .unwrap();
    // 桌面端用同一密钥加密一条快照响应，并绑定请求 nonce
    let plaintext = serde_json::to_vec(&message()).unwrap();
    let (response_nonce, ciphertext) = seal_response(&key, &nonce, &[9; 12], &plaintext).unwrap();
    let envelope = json!({"nonce": response_nonce, "ciphertext": ciphertext}).to_string();
    // 客户端校验并解密，得到摘要
    let record = response(
        &target,
        &request,
        200,
        Some("application/json; charset=utf-8"),
        envelope.as_bytes(),
    )
    .unwrap();
    // 接收时间取请求开始时间
    assert_eq!(record["receivedAtMs"], 5_000);
    // Hook 只保留公开字段
    assert_eq!(
        record["snapshot"]["hooks"][0],
        json!({"agent": "Codex", "slot": 2, "behavior": "running"})
    );

    // 换一次请求 nonce 后旧响应不可重放；状态码与类型也必须匹配。
    // 换一次请求的 nonce 后，旧响应不能重放
    let other = super::request(&target, 5_000).unwrap();
    assert!(
        response(
            &target,
            &other,
            200,
            Some("application/json"),
            envelope.as_bytes()
        )
        // 请求 nonce 不符：拒绝
        .is_err()
    );
    // 状态码不是 200 也拒绝
    assert!(
        response(
            &target,
            &request,
            500,
            Some("application/json"),
            envelope.as_bytes()
        )
        // 状态码非 200：拒绝
        .is_err()
    );
    // 媒体类型不是 JSON 同样拒绝
    assert!(
        response(
            &target,
            &request,
            200,
            Some("text/html"),
            envelope.as_bytes()
        )
        // 媒体类型非 JSON：拒绝
        .is_err()
    );
}

#[test]
// 主机只限局域网私有地址
fn hosts_are_limited_to_private_lan_addresses() {
    // 合法主机：私网/链路本地/回环 IPv4、带作用域的链路本地 IPv6、唯一本地 IPv6、.local 主机名
    for host in [
        "10.0.0.1",
        "172.16.4.2",
        "192.168.1.20",
        "169.254.3.4",
        "127.0.0.1",
        "fe80::1%en0",
        "fd00::2",
        // .local 主机名合法
        "mac-mini.local",
        // 末尾带点的 .local 也合法
        "studio.local.",
        // 逐个检查
    ] {
        // 应判定为合法
        assert!(valid_host(host), "{host}");
    }
    // 非法主机：公网地址、前导零、非 .local 域名、大写、公网 IPv6、非法作用域、空串
    for host in [
        "8.8.8.8",
        "192.168.01.2",
        "example.com",
        "Mac.local",
        "2001:db8::1",
        "fe80::1%EN0",
        "-a.local",
        // 空串
        "",
        // 逐个检查
    ] {
        // 应判定为非法
        assert!(!valid_host(host), "{host}");
    }
    // 取一份有效凭据作为基础
    let (_, target) = granted();
    // IPv6 作用域地址在 URL 里要加方括号，并把 % 转义为 %25
    let mut scoped = target.clone();
    scoped["host"] = json!("fe80::1%en0");
    assert_eq!(
        url(&scoped).unwrap(),
        "http://[fe80::1%25en0]:51303/api/widget/v1/snapshot"
    );
}

#[test]
// 凭据只来自声明了能力的消息
fn targets_come_only_from_declared_capability() {
    let (key, _) = granted();
    // 消息带有端点，但没有声明能力
    let mut message = message();
    message["widgetEndpoint"] = json!({"version": 1, "port": 51_303, "key": STANDARD.encode(key)});
    // 没声明 widgetHttpV1：不产生凭据
    assert!(target_from_message(&message, SOURCE, DEVICE, "192.168.1.20").is_none());
    // 声明能力后才产生
    message["capabilities"] = json!(["widgetHttpV1"]);
    let first = target_from_message(&message, SOURCE, DEVICE, "192.168.1.20").unwrap();
    let second = target_from_message(&message, SOURCE, DEVICE, "192.168.1.20").unwrap();
    // 每次授予都有新的代次
    assert_ne!(first["generation"], second["generation"]);
    // 但地址、设备与密钥相同，仍算同一连接
    assert!(same_connection(&first, &second));
    // 对端是公网地址：拒绝
    assert!(target_from_message(&message, SOURCE, DEVICE, "8.8.8.8").is_none());
}
