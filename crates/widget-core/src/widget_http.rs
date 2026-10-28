//! 系统扩展的独立只读摘要协议。凭据只用于加密，地址固定为局域网私有主机；
//! 原生层只执行一次 POST，请求体与响应校验全部在这里完成。
// 引入摘要
use crate::summary;
// 引入 base64
use base64::{Engine, engine::general_purpose::STANDARD};
// 引入认证加密
use chacha20poly1305::{
    // 加密算法、密钥与随机数类型
    ChaCha20Poly1305,
    // 密钥
    Key,
    // 密钥初始化
    KeyInit,
    // 随机数
    Nonce,
    // 加解密与载荷
    aead::{Aead, Payload},
};
// 引入 JSON
use serde_json::{Value, json};
// 引入 IP 地址类型
use std::net::{Ipv4Addr, Ipv6Addr};

// 请求超时 6 秒
pub const TIMEOUT_MS: u64 = 6_000;
// 响应上限 96 KiB
pub const MAX_RESPONSE_BYTES: usize = 96 * 1024;
// 固定的请求路径
pub const PATH: &str = "/api/widget/v1/snapshot";
// 固定的媒体类型
pub const MEDIA_TYPE: &str = "application/json";
// 请求与响应各自的上下文字节，用作加密的关联数据
const REQUEST_CONTEXT: &[u8] = b"loki-widget-request-v1";
// 响应的加密上下文
const RESPONSE_CONTEXT: &[u8] = b"loki-widget-response-v1";
// 认证标签长度
const TAG_BYTES: usize = 16;
// 凭据最大字节数
const MAX_TARGET_BYTES: usize = 4_096;

/// 已认证桌面授予的读取凭据：不含主连接配对秘密，也不接受任意 URL。
/// 构造时规范化主机并生成代次；同一连接重复授予保留原代次。
// 构造读取凭据：不含主连接的配对秘密，也不接受任意 URL；主机规范化并生成代次
pub fn target(source_id: &str, device_id: &str, host: &str, port: u64, key: &str) -> Option<Value> {
    // 构造凭据
    let value = json!({
        // 版本、来源、设备及其余字段
        "version": 1, "sourceId": source_id, "deviceId": device_id,
        // 主机小写、端口、密钥与代次
        "host": host.to_ascii_lowercase(), "port": port, "key": key, "generation": random_id()?,
    });
    // 有效才返回
    valid_target(&value).then_some(value)
}

/// 解密后的快照携带 `widgetEndpoint` 能力时，与实际对端地址组合为读取凭据。
// 解密后的快照带有 widgetHttpV1 能力时，与实际对端地址组合为读取凭据
pub fn target_from_message(
    // 消息
    message: &Value,
    // 来源标识
    source_id: &str,
    // 设备标识
    device_id: &str,
    // 对端地址
    peer: &str,
    // 返回凭据
) -> Option<Value> {
    // 能力列表
    let supported = message["capabilities"]
        // 转为数组
        .as_array()
        // 消息的能力列表里要有 widgetHttpV1
        .is_some_and(|items| items.iter().any(|item| item == "widgetHttpV1"));
    // 消息里声明的端点
    let endpoint = &message["widgetEndpoint"];
    // 未声明能力或端点版本不对
    if !supported || endpoint["version"].as_u64() != Some(1) {
        // 不产生凭据
        return None;
    }
    // 组合成读取凭据
    target(
        // 来源 ID
        source_id,
        // 本机设备 ID
        device_id,
        // 对端地址（来自已认证的套接字）
        peer,
        // 端点端口
        endpoint["port"].as_u64()?,
        // 读取密钥
        endpoint["key"].as_str()?,
    )
}

// 校验凭据：版本、ID、端口、主机与密钥
pub fn valid_target(value: &Value) -> bool {
    // 取字符串字段
    let text = |key: &str| value[key].as_str().unwrap_or_default();
    // 版本须为 1
    value["version"] == 1
        // 来源标识合法
        && summary::valid_id(text("sourceId"))
        // 设备标识合法
        && summary::valid_id(text("deviceId"))
        // 代次合法
        && summary::valid_id(text("generation"))
        // 来源与设备不同
        && text("sourceId") != text("deviceId")
        // 端口
        && value["port"]
            // 端口
            .as_u64()
            // 须在 1 到 65535
            .is_some_and(|port| (1..=65_535).contains(&port))
        // 主机合法
        && valid_host(text("host"))
        // 密钥合法
        && key_bytes(text("key")).is_some()
        // 序列化后不超过大小上限
        && serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= MAX_TARGET_BYTES)
}

/// 地址、设备或密钥变化才切换代次；重复同步不撤销在途请求。
// 地址、设备或密钥变化才算新连接；重复同步不撤销在途请求
pub fn same_connection(left: &Value, right: &Value) -> bool {
    // 左侧凭据有效
    valid_target(left)
        // 右侧凭据也有效
        && valid_target(right)
        // 这些字段两侧须一致
        && ["sourceId", "deviceId", "host", "port", "key"]
            // 遍历
            .iter()
            // 逐个比较
            .all(|key| left[key] == right[key])
}

/// 生成一次请求：固定地址与加密请求体。`nonce` 由调用方原样带回响应校验。
// 生成一次请求：固定地址与加密请求体；nonce 由调用方原样带回响应校验
pub fn request(target: &Value, now_ms: i64) -> Option<Value> {
    // 凭据无效或时间超出范围
    if !valid_target(target) || !(0..=i64::MAX - summary::LEASE_MS).contains(&now_ms) {
        // 返回 None
        return None;
    }
    // 解码密钥
    let key = key_bytes(target["key"].as_str()?)?;
    // 12 字节随机 nonce
    let mut nonce = [0u8; 12];
    // UUID v4 使用现有的操作系统随机数生成器;丢弃包含固定 UUID 位的字节。
    // 借用现有的系统随机数；跳过含固定 UUID 位的字节
    let random = uuid::Uuid::new_v4();
    // 逐字节填充随机数
    for (output, index) in nonce
        // 逐字节
        .iter_mut()
        // 与固定位置配对填充
        .zip([0, 1, 2, 3, 4, 5, 7, 9, 10, 11, 12, 13])
    {
        *output = random.as_bytes()[index];
    }
    // 加密固定的请求内容
    let sealed = ChaCha20Poly1305::new(&Key::from(key))
        // 加密
        .encrypt(
            // 随机数
            &Nonce::from(nonce),
            // 载荷
            Payload {
                // 附加数据：请求上下文
                msg: REQUEST_CONTEXT,
                // 附加数据：请求上下文
                aad: REQUEST_CONTEXT,
            },
        )
        // 加密失败则返回 None
        .ok()?;
    // 请求体
    let body = json!({
        // 客户端与设备标识
        "clientId": target["sourceId"], "deviceId": target["deviceId"],
        // nonce 与密文（base64）
        "nonce": STANDARD.encode(nonce), "ciphertext": STANDARD.encode(sealed),
    });
    // 整个请求描述
    Some(json!({
        // 地址、方法、媒体类型与超时
        "url": url(target)?, "method": "POST", "mediaType": MEDIA_TYPE, "timeoutMs": TIMEOUT_MS,
        // 响应大小上限与请求体
        "maxResponseBytes": MAX_RESPONSE_BYTES, "body": body.to_string(),
        // nonce 与开始时间，供响应校验使用
        "nonce": STANDARD.encode(nonce), "startedAtMs": now_ms,
    }))
}

/// 校验状态、类型与大小，解密并绑定本次请求 nonce，产出带初始租约的摘要。
// 校验状态、类型与大小，解密并绑定本次请求的 nonce，产出带初始租约的摘要
pub fn response(
    // 凭据
    target: &Value,
    // 请求
    request: &Value,
    // 状态码
    status: u64,
    // 媒体类型
    media_type: Option<&str>,
    // 响应体
    body: &[u8],
    // 返回消息
) -> Result<Value, String> {
    // 统一的错误
    let invalid = || "小组件摘要响应无效".to_owned();
    // 只比较分号前的媒体类型
    let media = media_type
        // 缺省为空串
        .unwrap_or_default()
        // 按分号拆分
        .split(';')
        // 取第一段
        .next()
        // 缺省为空串
        .unwrap_or_default()
        // 去掉空白
        .trim();
    // 状态必须是 200、类型正确、大小不超限
    if status != 200 || !media.eq_ignore_ascii_case(MEDIA_TYPE) || body.len() > MAX_RESPONSE_BYTES {
        // 返回错误
        return Err(invalid());
    }
    // 解码密钥
    let key = key_bytes(target["key"].as_str().unwrap_or_default()).ok_or_else(invalid)?;
    // 请求 nonce
    let request_nonce = decode(request["nonce"].as_str(), 12).ok_or_else(invalid)?;
    // 解析响应信封
    let envelope: Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    // 响应 nonce
    let nonce: [u8; 12] = decode(envelope["nonce"].as_str(), 12)
        // 转为定长数组
        .and_then(|bytes| bytes.try_into().ok())
        // nonce 无效则报错
        .ok_or_else(invalid)?;
    // 解码密文
    let ciphertext = STANDARD
        // 取出 base64 文本
        .decode(envelope["ciphertext"].as_str().unwrap_or_default())
        // 解码失败则报错
        .map_err(|_| invalid())?;
    // 密文长度必须在合理范围
    if !(TAG_BYTES..=summary::MAX_BYTES + TAG_BYTES).contains(&ciphertext.len()) {
        // 返回错误
        return Err(invalid());
    }
    // 关联数据绑定本次请求的 nonce
    let mut aad = RESPONSE_CONTEXT.to_vec();
    // 附加数据追加请求 nonce
    aad.extend_from_slice(&request_nonce);
    // 解密
    let plaintext = ChaCha20Poly1305::new(&Key::from(key))
        // 解密
        .decrypt(
            // 随机数
            &Nonce::from(nonce),
            // 载荷
            Payload {
                // 密文
                msg: &ciphertext,
                // 附加数据
                aad: &aad,
            },
        )
        // 解密失败则报错
        .map_err(|_| invalid())?;
    // 解析为 JSON 消息
    let message: Value = serde_json::from_slice(&plaintext).map_err(|_| invalid())?;
    // 凭据里记录的来源
    let source = target["sourceId"].as_str().unwrap_or_default();
    // 必须是来自目标来源的快照
    if message["type"] != "snapshot" || message["sourceId"] != source {
        // 返回错误
        return Err(invalid());
    }
    // 以请求开始时间作为接收时间
    let started = request["startedAtMs"].as_i64().ok_or_else(invalid)?;
    // 由消息构造摘要
    summary::from_message(&message, source, started).ok_or_else(invalid)
}

// 拼出固定的 HTTP 地址；IPv6 字面量加方括号并转义作用域符号
fn url(target: &Value) -> Option<String> {
    // 主机
    let host = target["host"].as_str()?;
    // IPv6 地址加方括号
    let host = if host.contains(':') {
        // 作用域的百分号需转义
        format!("[{}]", host.replace('%', "%25"))
    // 否则分支
    } else {
        // 其余原样
        host.to_owned()
    };
    // 拼出请求地址
    Some(format!("http://{host}:{}{PATH}", target["port"].as_u64()?))
}

// 密钥必须是 32 字节的 base64
fn key_bytes(text: &str) -> Option<[u8; 32]> {
    // 解码为 32 字节密钥
    decode(Some(text), 32)?.try_into().ok()
}

// 解码 base64 并检查长度
fn decode(text: Option<&str>, length: usize) -> Option<Vec<u8>> {
    // Base64 解码
    STANDARD
        // 解码
        .decode(text?)
        // 失败为 None
        .ok()
        // 长度须一致
        .filter(|bytes| bytes.len() == length)
}

// 随机代次（UUID v4）
fn random_id() -> Option<String> {
    // 随机 UUID
    Some(uuid::Uuid::new_v4().to_string())
}

/// 只允许局域网私有 IPv4、带可选作用域的私有 IPv6 字面量与 `.local` 主机名。
// 只允许局域网私有 IPv4、带可选作用域的私有 IPv6 字面量与 .local 主机名
pub fn valid_host(host: &str) -> bool {
    // 必须非空、不超过 253 字节、全小写且只含可见 ASCII
    if host.is_empty()
        // 或者主机名过长
        || host.len() > 253
        // 或者不是小写
        || host != host.to_ascii_lowercase()
        // 或者含不可见字符
        || !host.bytes().all(|b| (33..=126).contains(&b))
    {
        // 无效
        return false;
    }
    // IPv6：可带作用域，作用域限制为小写字母数字与 -._
    if host.contains(':') {
        // 按百分号拆为地址与作用域
        let mut parts = host.splitn(2, '%');
        // 地址部分
        let address = parts.next().unwrap_or_default();
        // 有作用域
        if let Some(scope) = parts.next() {
            // 长度须在 1 到 32
            let valid = (1..=32).contains(&scope.len())
                // 并且作用域
                && scope
                    // 逐字节
                    .bytes()
                    // 只含小写字母、数字和 -._
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-._".contains(&b));
            // 作用域不合法
            if !valid {
                // 整个主机不合法
                return false;
            }
        }
        // 地址部分必须是私有 IPv6
        return address.parse::<Ipv6Addr>().is_ok_and(private_ipv6);
    }
    // IPv4：不接受前导零等非规范写法，避免不同解析器得到不同地址
    if host.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        // 不接受前导零等非规范写法，避免不同解析器得到不同地址。
        return host
            // 解析为 IPv4
            .parse::<Ipv4Addr>()
            // 写法须规范且为私有地址
            .is_ok_and(|address| address.to_string() == host && private_ipv4(address));
    }
    // 其余必须是合法的 .local 主机名
    let name = host.strip_suffix('.').unwrap_or(host);
    // 须以 .local 结尾
    name.ends_with(".local")
        // 至少两段
        && name.split('.').count() >= 2
        // 逐段检查
        && name.split('.').all(|label| {
            // 标签非空
            !label.is_empty()
                // 每段不超过 63 字符
                && label.len() <= 63
                // 不以连字符开头
                && !label.starts_with('-')
                // 不以连字符结尾
                && !label.ends_with('-')
                // 标签只含小写字母、数字与连字符
                && label
                    // 逐字节检查
                    .bytes()
                    // 不合格则整个主机名无效
                    .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase() || b == b'-')
        })
}

// 私有、回环或链路本地
fn private_ipv4(address: Ipv4Addr) -> bool {
    // 私有、回环或链路本地地址
    address.is_private() || address.is_loopback() || address.is_link_local()
}

// IPv4 映射地址按 IPv4 判断；其余允许回环、唯一本地与链路本地
fn private_ipv6(address: Ipv6Addr) -> bool {
    // IPv4 映射地址
    if let Some(mapped) = address.to_ipv4_mapped() {
        // 按 IPv4 判断
        return private_ipv4(mapped);
    }
    // 首段
    let first = address.segments()[0];
    // 回环、唯一本地或链路本地
    address.is_loopback() || first & 0xfe00 == 0xfc00 || first & 0xffc0 == 0xfe80
}

// 测试模块
#[cfg(test)]
#[path = "widget_http_test.rs"]
mod widget_http_test;
