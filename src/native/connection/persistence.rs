//! owner 的持久化边界：身份与名称、系统安全存储中的配对记录、离线快照缓存。
//! 配对秘密只进入系统安全存储；其余文件位于应用私有目录并原子替换。
use super::{
    Config, MAX_FRAME, Material, err, owner::Owner, snapshot_admission::validate_snapshot,
};
use crate::features::connection::identity::DEFAULT_NAME;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use loki_metris_pairing::{valid_device_id, valid_device_name};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

/// 设备身份与名称。宿主自有身份时以宿主为准；否则先用已保存身份，再用传入值，最后生成新 UUID。
/// 名称一旦由 owner 保存，以保存值为准；传入值只在首次启动时生效。
pub(super) fn identity(root: &Path, config: &Config) -> Result<(String, String), String> {
    let path = root.join("identity.json");
    let stored = fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .unwrap_or(Value::Null);
    let stored_id = stored["deviceId"].as_str().filter(|v| valid_device_id(v));
    let given_id = valid_device_id(&config.device_id).then_some(config.device_id.as_str());
    let id = if config.host_identity {
        given_id.ok_or("设备身份无效")?.to_owned()
    } else {
        stored_id
            .or(given_id)
            .map_or_else(|| uuid::Uuid::new_v4().to_string(), str::to_owned)
    };
    let stored_name = stored["name"]
        .as_str()
        .filter(|n| stored_id == Some(id.as_str()) && valid_device_name(n));
    let name = stored_name
        .or_else(|| valid_device_name(&config.name).then_some(config.name.as_str()))
        .unwrap_or(DEFAULT_NAME)
        .to_owned();
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
pub(super) fn decode_secret(text: &str) -> Option<[u8; 32]> {
    let raw = STANDARD.decode(text).ok()?;
    (STANDARD.encode(&raw) == text).then_some(())?;
    raw.try_into().ok()
}

/// Read only this application's current pairing book; retired namespaces are never imported.
pub(super) fn load_records(
    service: &str,
    device: &str,
) -> Result<BTreeMap<String, Material>, String> {
    let records: BTreeMap<String, Material> =
        match crate::native::credentials::read(service, device)? {
            Some(text) => {
                serde_json::from_str(&text).map_err(|_| "安全存储记录损坏，请重新配对。")?
            }
            None => BTreeMap::new(),
        };
    if records.len() > 64
        || records.iter().any(|(key, v)| {
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

fn write_records(
    service: &str,
    device: &str,
    records: &BTreeMap<String, Material>,
) -> Result<(), String> {
    if records.is_empty() {
        crate::native::credentials::delete(service, device)
    } else {
        crate::native::credentials::write(
            service,
            device,
            &serde_json::to_string(records).map_err(err)?,
        )
    }
}

impl Owner {
    pub(super) fn save_records(&self, records: &BTreeMap<String, Material>) -> Result<(), String> {
        write_records(&self.credential_service, &self.id, records)
    }

    pub(super) fn write_snapshot(&self) {
        let _ = serde_json::to_vec(&json!({"sourceId":self.selected,"receivedAtMs":self.view["receivedAtMs"],"snapshot":self.view["snapshot"]}))
            .map_err(err)
            .and_then(|bytes| atomic_write(&self.root.join("snapshot.json"), &bytes));
    }

    /// 只恢复仍受信任来源的合法快照；它带原接收时间，不能显示为实时。
    pub(super) fn restore_cache(&mut self) {
        if !self.keep_snapshot {
            erase_snapshot(&self.root);
            return;
        }
        let Some(v) = fs::read(self.root.join("snapshot.json"))
            .ok()
            .filter(|b| b.len() <= MAX_FRAME)
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        else {
            return;
        };
        let Some(id) = v["sourceId"]
            .as_str()
            .filter(|id| self.records.contains_key(*id))
        else {
            return;
        };
        if validate_snapshot(&v["snapshot"]).is_err() || v["receivedAtMs"].as_u64().is_none() {
            return;
        }
        self.selected = Some(id.to_owned());
        self.view["selectedId"] = json!(id);
        self.view["snapshot"] = v["snapshot"].clone();
        self.view["receivedAtMs"] = v["receivedAtMs"].clone();
        self.view["cacheSourceId"] = json!(id);
        self.view["status"] = json!("显示最近缓存，等待桌面连接…");
        self.restore_cached_images();
    }
}

pub(super) fn erase_snapshot(root: &Path) {
    let _ = fs::remove_file(root.join("snapshot.json"));
}

pub(super) fn private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(err)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(err)?;
    }
    Ok(())
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("pending");
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp).map_err(err)?;
    file.write_all(bytes).map_err(err)?;
    file.sync_all().map_err(err)?;
    drop(file);
    fs::rename(&temp, path).map_err(err)?;
    Ok(())
}

/// 桌面后端按 URI 解码路径；移动后端直接去掉 `file://` 前缀，不解码。
pub(super) fn file_url(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if cfg!(any(target_os = "android", target_os = "ios")) {
        return format!("file://{raw}");
    }
    let encoded: String = raw
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/:-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}
