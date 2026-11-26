//! owner 的持久化边界：身份与名称、系统安全存储中的配对记录、离线快照缓存。
//! 配对秘密只进入系统安全存储；其余文件位于应用私有目录并原子替换。
use super::{Config, Material, err};
use crate::features::connection::identity::DEFAULT_NAME;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use loki_metris_pairing::{valid_device_id, valid_device_name};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    // 文件操作与打开选项
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

/// 设备身份与名称。宿主自有身份时以宿主为准；否则先用已保存身份，再用传入值，最后生成新 UUID。
/// 名称一旦由 owner 保存，以保存值为准；传入值只在首次启动时生效。
// 设备身份与名称：宿主自有身份以宿主为准；否则先用已保存身份，再用传入值，最后生成新 UUID。名称以 owner 保存的值为准，传入值只在首次启动生效
pub(super) fn identity(root: &Path, config: &Config) -> Result<(String, String), String> {
    let path = root.join("identity.json");
    // 读取已保存的身份文件，损坏视为没有
    let stored = fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .unwrap_or(Value::Null);
    // 已保存的设备标识，须合法
    let stored_id = stored["deviceId"].as_str().filter(|v| valid_device_id(v));
    // 传入的 ID 合法才采用
    let given_id = valid_device_id(&config.device_id).then_some(config.device_id.as_str());
    // 宿主身份优先
    let id = if config.host_identity {
        // 宿主身份必须合法
        given_id.ok_or("设备身份无效")?.to_owned()
    // 否则分支
    } else {
        // 否则沿用已保存的
        stored_id
            // 否则使用给定标识
            .or(given_id)
            // 仍没有则新建 UUID
            .map_or_else(|| uuid::Uuid::new_v4().to_string(), str::to_owned)
    };
    // 已保存名称仅在身份一致且合法时采用
    let stored_name = stored["name"]
        .as_str()
        // 标识一致且名称合法才沿用
        .filter(|n| stored_id == Some(id.as_str()) && valid_device_name(n));
    // 依次尝试：已保存名称、传入名称、默认名称
    let name = stored_name
        // 否则用配置里的名称
        .or_else(|| valid_device_name(&config.name).then_some(config.name.as_str()))
        // 仍不合法则用默认名
        .unwrap_or(DEFAULT_NAME)
        .to_owned();
    // 写回规范化后的身份
    atomic_write(
        &path,
        &serde_json::to_vec(&json!({"deviceId":id,"name":name})).map_err(err)?,
    )?;
    Ok((id, name))
}

pub(super) fn save_name(root: &Path, id: &str, name: &str) -> Result<(), String> {
    atomic_write(
        &root.join("identity.json"),
        &serde_json::to_vec(&json!({"deviceId":id,"name":name})).map_err(err)?,
    )
}

/// 规范 Base64 的 32 字节秘密；其他编码一律视为损坏记录。
// 解码规范 Base64 的 32 字节秘密；其他编码一律视为损坏记录
pub(super) fn decode_secret(text: &str) -> Option<[u8; 32]> {
    // Base64 解码
    let raw = STANDARD.decode(text).ok()?;
    // 重新编码必须与原文一致，排除非规范写法
    (STANDARD.encode(&raw) == text).then_some(())?;
    raw.try_into().ok()
}

/// 只读取本应用当前的配对簿;已退役的命名空间绝不导入。
// 只读取本应用当前的配对簿，已退役的命名空间从不导入
pub(super) fn load_records(
    service: &str,
    device: &str,
) -> Result<BTreeMap<String, Material>, String> {
    let records: BTreeMap<String, Material> =
        match crate::native::credentials::read(service, device)? {
            Some(text) => {
                // 解析 JSON，损坏则要求重新配对
                serde_json::from_str(&text).map_err(|_| "安全存储记录损坏，请重新配对。")?
            }
            None => BTreeMap::new(),
        };
    // 数量上限 64；键与客户端 ID 一致、设备一致、秘密可解码
    if records.len() > 64
        // 或者有任一记录无效
        || records.iter().any(|(key, v)| {
            // 排除要删除的客户端
            key != &v.client_id
                || v.device_id != device
                || !valid_device_id(key)
                || decode_secret(&v.secret).is_none()
        })
    {
        return Err("安全存储记录无效".into());
    }
    Ok(records)
}

// 写入配对簿；为空则删除凭据条目
pub(super) fn write_records(
    service: &str,
    device: &str,
    records: &BTreeMap<String, Material>,
) -> Result<(), String> {
    if records.is_empty() {
        crate::native::credentials::delete(service, device)
    // 否则分支
    } else {
        crate::native::credentials::write(
            service,
            device,
            &serde_json::to_string(records).map_err(err)?,
        )
    }
}

pub(super) fn erase_snapshot(root: &Path) {
    let _ = fs::remove_file(root.join("snapshot.json"));
}

// 创建目录并在 Unix 上设为仅所有者可访问（0700）
pub(super) fn private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(err)?;
    #[cfg(unix)]
    {
        // Unix 权限扩展
        use std::os::unix::fs::PermissionsExt;
        // 设为仅所有者可访问
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(err)?;
    }
    Ok(())
}

// 原子写入：先写临时文件并同步到磁盘，再改名覆盖
pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("pending");
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    // Unix 上文件权限设为 0600
    #[cfg(unix)]
    {
        // Unix 打开选项扩展
        use std::os::unix::fs::OpenOptionsExt;
        // 仅所有者可读写
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(err)?;
    file.write_all(bytes).map_err(err)?;
    file.sync_all().map_err(err)?;
    drop(file);
    // 原子改名到目标位置
    fs::rename(&temp, path).map_err(err)?;
    Ok(())
}

/// 桌面后端按 URI 解码路径；移动后端直接去掉 `file://` 前缀，不解码。
// 桌面后端按 URI 解码路径；移动后端直接去掉 file:// 前缀，不解码
pub(super) fn file_url(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if cfg!(any(target_os = "android", target_os = "ios")) {
        // Unix 路径直接加协议头
        return format!("file://{raw}");
    }
    // 其余平台对非安全字符做百分号编码
    let encoded: String = raw
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/:-_.~".contains(&b) {
                (b as char).to_string()
            // 否则分支
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    // 绝对路径补 file:// ，Windows 盘符路径补 file:///
    if encoded.starts_with('/') {
        // Unix 路径格式
        format!("file://{encoded}")
    // 否则分支
    } else {
        // Windows 路径格式
        format!("file:///{encoded}")
    }
}
