//! 系统扩展的独立只读摘要协议。凭据只用于加密，地址固定为局域网私有主机；
//! 原生层只执行一次 POST，请求体与响应校验全部在这里完成。
use crate::summary;
use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use serde_json::{Value, json};
use std::net::{Ipv4Addr, Ipv6Addr};

pub const TIMEOUT_MS: u64 = 6_000;
pub const MAX_RESPONSE_BYTES: usize = 96 * 1024;
pub const PATH: &str = "/api/widget/v1/snapshot";
pub const MEDIA_TYPE: &str = "application/json";
const REQUEST_CONTEXT: &[u8] = b"loki-widget-request-v1";
const RESPONSE_CONTEXT: &[u8] = b"loki-widget-response-v1";
const TAG_BYTES: usize = 16;
const MAX_TARGET_BYTES: usize = 4_096;

/// 已认证桌面授予的读取凭据：不含主连接配对秘密，也不接受任意 URL。
/// 构造时规范化主机并生成代次；同一连接重复授予保留原代次。
pub fn target(source_id: &str, device_id: &str, host: &str, port: u64, key: &str) -> Option<Value> {
    let value = json!({
        "version": 1, "sourceId": source_id, "deviceId": device_id,
        "host": host.to_ascii_lowercase(), "port": port, "key": key, "generation": random_id()?,
    });
    valid_target(&value).then_some(value)
}

/// 解密后的快照携带 `widgetEndpoint` 能力时，与实际对端地址组合为读取凭据。
pub fn target_from_message(
    message: &Value,
    source_id: &str,
    device_id: &str,
    peer: &str,
) -> Option<Value> {
    let supported = message["capabilities"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item == "widgetHttpV1"));
    let endpoint = &message["widgetEndpoint"];
    if !supported || endpoint["version"].as_u64() != Some(1) {
        return None;
    }
    target(
        source_id,
        device_id,
        peer,
        endpoint["port"].as_u64()?,
        endpoint["key"].as_str()?,
    )
}

pub fn valid_target(value: &Value) -> bool {
    let text = |key: &str| value[key].as_str().unwrap_or_default();
    value["version"] == 1
        && summary::valid_id(text("sourceId"))
        && summary::valid_id(text("deviceId"))
        && summary::valid_id(text("generation"))
        && text("sourceId") != text("deviceId")
        && value["port"]
            .as_u64()
            .is_some_and(|port| (1..=65_535).contains(&port))
        && valid_host(text("host"))
        && key_bytes(text("key")).is_some()
        && serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= MAX_TARGET_BYTES)
}

/// 地址、设备或密钥变化才切换代次；重复同步不撤销在途请求。
pub fn same_connection(left: &Value, right: &Value) -> bool {
    valid_target(left)
        && valid_target(right)
        && ["sourceId", "deviceId", "host", "port", "key"]
            .iter()
            .all(|key| left[key] == right[key])
}

/// 生成一次请求：固定地址与加密请求体。`nonce` 由调用方原样带回响应校验。
pub fn request(target: &Value, now_ms: i64) -> Option<Value> {
    if !valid_target(target) || !(0..=i64::MAX - summary::LEASE_MS).contains(&now_ms) {
        return None;
    }
    let key = key_bytes(target["key"].as_str()?)?;
    let mut nonce = [0u8; 12];
    // UUID v4 uses the existing OS RNG; omit bytes containing fixed UUID bits.
    let random = uuid::Uuid::new_v4();
    for (output, index) in nonce
        .iter_mut()
        .zip([0, 1, 2, 3, 4, 5, 7, 9, 10, 11, 12, 13])
    {
        *output = random.as_bytes()[index];
    }
    let sealed = ChaCha20Poly1305::new(&Key::from(key))
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: REQUEST_CONTEXT,
                aad: REQUEST_CONTEXT,
            },
        )
        .ok()?;
    let body = json!({
        "clientId": target["sourceId"], "deviceId": target["deviceId"],
        "nonce": STANDARD.encode(nonce), "ciphertext": STANDARD.encode(sealed),
    });
    Some(json!({
        "url": url(target)?, "method": "POST", "mediaType": MEDIA_TYPE, "timeoutMs": TIMEOUT_MS,
        "maxResponseBytes": MAX_RESPONSE_BYTES, "body": body.to_string(),
        "nonce": STANDARD.encode(nonce), "startedAtMs": now_ms,
    }))
}

/// 校验状态、类型与大小，解密并绑定本次请求 nonce，产出带初始租约的摘要。
pub fn response(
    target: &Value,
    request: &Value,
    status: u64,
    media_type: Option<&str>,
    body: &[u8],
) -> Result<Value, String> {
    let invalid = || "小组件摘要响应无效".to_owned();
    let media = media_type
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim();
    if status != 200 || !media.eq_ignore_ascii_case(MEDIA_TYPE) || body.len() > MAX_RESPONSE_BYTES {
        return Err(invalid());
    }
    let key = key_bytes(target["key"].as_str().unwrap_or_default()).ok_or_else(invalid)?;
    let request_nonce = decode(request["nonce"].as_str(), 12).ok_or_else(invalid)?;
    let envelope: Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    let nonce: [u8; 12] = decode(envelope["nonce"].as_str(), 12)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(invalid)?;
    let ciphertext = STANDARD
        .decode(envelope["ciphertext"].as_str().unwrap_or_default())
        .map_err(|_| invalid())?;
    if !(TAG_BYTES..=summary::MAX_BYTES + TAG_BYTES).contains(&ciphertext.len()) {
        return Err(invalid());
    }
    let mut aad = RESPONSE_CONTEXT.to_vec();
    aad.extend_from_slice(&request_nonce);
    let plaintext = ChaCha20Poly1305::new(&Key::from(key))
        .decrypt(
            &Nonce::from(nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| invalid())?;
    let message: Value = serde_json::from_slice(&plaintext).map_err(|_| invalid())?;
    let source = target["sourceId"].as_str().unwrap_or_default();
    if message["type"] != "snapshot" || message["sourceId"] != source {
        return Err(invalid());
    }
    let started = request["startedAtMs"].as_i64().ok_or_else(invalid)?;
    summary::from_message(&message, source, started).ok_or_else(invalid)
}

fn url(target: &Value) -> Option<String> {
    let host = target["host"].as_str()?;
    let host = if host.contains(':') {
        format!("[{}]", host.replace('%', "%25"))
    } else {
        host.to_owned()
    };
    Some(format!("http://{host}:{}{PATH}", target["port"].as_u64()?))
}

fn key_bytes(text: &str) -> Option<[u8; 32]> {
    decode(Some(text), 32)?.try_into().ok()
}

fn decode(text: Option<&str>, length: usize) -> Option<Vec<u8>> {
    STANDARD
        .decode(text?)
        .ok()
        .filter(|bytes| bytes.len() == length)
}

fn random_id() -> Option<String> {
    Some(uuid::Uuid::new_v4().to_string())
}

/// 只允许局域网私有 IPv4、带可选作用域的私有 IPv6 字面量与 `.local` 主机名。
pub fn valid_host(host: &str) -> bool {
    if host.is_empty()
        || host.len() > 253
        || host != host.to_ascii_lowercase()
        || !host.bytes().all(|b| (33..=126).contains(&b))
    {
        return false;
    }
    if host.contains(':') {
        let mut parts = host.splitn(2, '%');
        let address = parts.next().unwrap_or_default();
        if let Some(scope) = parts.next() {
            let valid = (1..=32).contains(&scope.len())
                && scope
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-._".contains(&b));
            if !valid {
                return false;
            }
        }
        return address.parse::<Ipv6Addr>().is_ok_and(private_ipv6);
    }
    if host.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        // 不接受前导零等非规范写法，避免不同解析器得到不同地址。
        return host
            .parse::<Ipv4Addr>()
            .is_ok_and(|address| address.to_string() == host && private_ipv4(address));
    }
    let name = host.strip_suffix('.').unwrap_or(host);
    name.ends_with(".local")
        && name.split('.').count() >= 2
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase() || b == b'-')
        })
}

fn private_ipv4(address: Ipv4Addr) -> bool {
    address.is_private() || address.is_loopback() || address.is_link_local()
}

fn private_ipv6(address: Ipv6Addr) -> bool {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return private_ipv4(mapped);
    }
    let first = address.segments()[0];
    address.is_loopback() || first & 0xfe00 == 0xfc00 || first & 0xffc0 == 0xfe80
}

#[cfg(test)]
#[path = "widget_http_test.rs"]
mod widget_http_test;
