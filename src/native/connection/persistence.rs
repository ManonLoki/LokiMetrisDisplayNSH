//! owner 的持久化边界：身份与名称、系统安全存储中的配对记录、离线快照缓存。
//! 配对秘密只进入系统安全存储；其余文件位于应用私有目录并原子替换。
// 引入配置、常量、材料、错误、owner 与快照校验
use super::{
    // 配置、帧上限、密钥材料与快照校验
    Config,
    // 帧上限
    MAX_FRAME,
    // 密钥材料
    Material,
    // 错误转换
    err,
    // 所有者
    owner::Owner,
    // 快照校验
    snapshot_admission::validate_snapshot,
};
// 引入默认名称
use crate::features::connection::identity::DEFAULT_NAME;
// 引入 base64
use base64::{Engine as _, engine::general_purpose::STANDARD};
// 引入 ID 与名称校验
use loki_metris_pairing::{valid_device_id, valid_device_name};
// 引入 JSON
use serde_json::{Value, json};
// 引入集合与文件系统
use std::{
    // 有序映射
    collections::BTreeMap,
    // 文件操作与打开选项
    fs::{self, OpenOptions},
    // 写特征
    io::Write,
    // 路径类型
    path::Path,
};

/// 设备身份与名称。宿主自有身份时以宿主为准；否则先用已保存身份，再用传入值，最后生成新 UUID。
/// 名称一旦由 owner 保存，以保存值为准；传入值只在首次启动时生效。
// 设备身份与名称：宿主自有身份以宿主为准；否则先用已保存身份，再用传入值，最后生成新 UUID。名称以 owner 保存的值为准，传入值只在首次启动生效
pub(super) fn identity(root: &Path, config: &Config) -> Result<(String, String), String> {
    // 身份文件路径
    let path = root.join("identity.json");
    // 读取已保存的身份文件，损坏视为没有
    let stored = fs::read(&path)
        // 读取成功才继续
        .ok()
        // 解析为 JSON
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        // 失败为 null
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
        // 取字符串
        .as_str()
        // 标识一致且名称合法才沿用
        .filter(|n| stored_id == Some(id.as_str()) && valid_device_name(n));
    // 依次尝试：已保存名称、传入名称、默认名称
    let name = stored_name
        // 否则用配置里的名称
        .or_else(|| valid_device_name(&config.name).then_some(config.name.as_str()))
        // 仍不合法则用默认名
        .unwrap_or(DEFAULT_NAME)
        // 转为自有字符串
        .to_owned();
    // 写回规范化后的身份
    atomic_write(
        // 路径
        &path,
        // 序列化身份
        &serde_json::to_vec(&json!({"deviceId":id,"name":name})).map_err(err)?,
        // 写入完成
    )?;
    // 返回标识与名称
    Ok((id, name))
}

// 保存新的设备名称
pub(super) fn save_name(root: &Path, id: &str, name: &str) -> Result<(), String> {
    // 原子写入
    atomic_write(
        // 身份文件路径
        &root.join("identity.json"),
        // 序列化身份
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
    // 转为定长数组
    raw.try_into().ok()
}

/// 只读取本应用当前的配对簿;已退役的命名空间绝不导入。
// 只读取本应用当前的配对簿，已退役的命名空间从不导入
pub(super) fn load_records(
    // 服务名
    service: &str,
    // 设备标识
    device: &str,
    // 返回记录表
) -> Result<BTreeMap<String, Material>, String> {
    // 解析为配对记录表
    let records: BTreeMap<String, Material> =
        // 读取系统凭据
        match crate::native::credentials::read(service, device)? {
            // 有内容
            Some(text) => {
                // 解析 JSON，损坏则要求重新配对
                serde_json::from_str(&text).map_err(|_| "安全存储记录损坏，请重新配对。")?
            }
            // 没有记录则为空表
            None => BTreeMap::new(),
        };
    // 数量上限 64；键与客户端 ID 一致、设备一致、秘密可解码
    if records.len() > 64
        // 或者有任一记录无效
        || records.iter().any(|(key, v)| {
            // 排除要删除的客户端
            key != &v.client_id
                // 设备标识不符
                || v.device_id != device
                // 客户端标识不合法
                || !valid_device_id(key)
                // 密钥无法解码
                || decode_secret(&v.secret).is_none()
        })
    {
        // 记录无效
        return Err("安全存储记录无效".into());
    }
    // 返回记录表
    Ok(records)
}

// 写入配对簿；为空则删除凭据条目
fn write_records(
    // 服务名
    service: &str,
    // 设备标识
    device: &str,
    // 记录表
    records: &BTreeMap<String, Material>,
    // 返回结果
) -> Result<(), String> {
    // 记录为空
    if records.is_empty() {
        // 删除系统凭据
        crate::native::credentials::delete(service, device)
    // 否则分支
    } else {
        // 写入系统凭据
        crate::native::credentials::write(
            // 服务名
            service,
            // 设备标识
            device,
            // 序列化记录
            &serde_json::to_string(records).map_err(err)?,
        )
    }
}

// 所有者的方法
impl Owner {
    // 保存配对簿到系统安全存储
    pub(super) fn save_records(&self, records: &BTreeMap<String, Material>) -> Result<(), String> {
        // 写入记录表
        write_records(&self.credential_service, &self.id, records)
    }

    // 把最近的快照连同来源与接收时间写入缓存文件，失败忽略
    pub(super) fn write_snapshot(&self) {
        // 估算快照缓存的大小
        let _ = serde_json::to_vec(&json!({"sourceId":self.selected,"receivedAtMs":self.view["receivedAtMs"],"snapshot":self.view["snapshot"]}))
            // 失败转为错误
            .map_err(err)
            // 原子写入快照文件
            .and_then(|bytes| atomic_write(&self.root.join("snapshot.json"), &bytes));
    }

    /// 只恢复仍受信任来源的合法快照；它带原接收时间，不能显示为实时。
    // 只恢复仍受信任来源的合法快照；它带原接收时间，不能显示为实时
    pub(super) fn restore_cache(&mut self) {
        // 不保留快照的宿主清掉旧缓存
        if !self.keep_snapshot {
            // 删除快照缓存
            erase_snapshot(&self.root);
            // 返回
            return;
        }
        // 读取缓存，大小不得超过单帧上限
        let Some(v) = fs::read(self.root.join("snapshot.json"))
            // 读取成功才继续
            .ok()
            // 不超过帧上限
            .filter(|b| b.len() <= MAX_FRAME)
            // 解析为 JSON
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        // 否则忽略
        else {
            // 返回
            return;
        };
        // 来源必须仍有配对记录
        let Some(id) = v["sourceId"]
            // 取字符串
            .as_str()
            // 须在配对记录里
            .filter(|id| self.records.contains_key(*id))
        // 否则忽略
        else {
            // 返回
            return;
        };
        // 快照必须通过准入校验且有接收时间
        if validate_snapshot(&v["snapshot"]).is_err() || v["receivedAtMs"].as_u64().is_none() {
            // 返回
            return;
        }
        // 恢复选中的桌面与展示内容
        self.selected = Some(id.to_owned());
        // 选中项
        self.view["selectedId"] = json!(id);
        // 快照
        self.view["snapshot"] = v["snapshot"].clone();
        // 接收时间
        self.view["receivedAtMs"] = v["receivedAtMs"].clone();
        // 缓存来源
        self.view["cacheSourceId"] = json!(id);
        // 状态：显示最近缓存，等待桌面连接
        self.view["status"] = json!("显示最近缓存，等待桌面连接…");
        // 同时恢复本地已校验的图片
        self.restore_cached_images();
    }
}

// 删除快照缓存
pub(super) fn erase_snapshot(root: &Path) {
    // 删除快照文件
    let _ = fs::remove_file(root.join("snapshot.json"));
}

// 创建目录并在 Unix 上设为仅所有者可访问（0700）
pub(super) fn private_dir(path: &Path) -> Result<(), String> {
    // 创建目录
    fs::create_dir_all(path).map_err(err)?;
    #[cfg(unix)]
    {
        // Unix 权限扩展
        use std::os::unix::fs::PermissionsExt;
        // 设为仅所有者可访问
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(err)?;
    }
    // 完成
    Ok(())
}

// 原子写入：先写临时文件并同步到磁盘，再改名覆盖
pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    // 临时文件路径
    let temp = path.with_extension("pending");
    // 打开选项
    let mut options = OpenOptions::new();
    // 可写、创建并截断
    options.write(true).create(true).truncate(true);
    // Unix 上文件权限设为 0600
    #[cfg(unix)]
    {
        // Unix 打开选项扩展
        use std::os::unix::fs::OpenOptionsExt;
        // 仅所有者可读写
        options.mode(0o600);
    }
    // 打开临时文件
    let mut file = options.open(&temp).map_err(err)?;
    // 写入内容
    file.write_all(bytes).map_err(err)?;
    // 刷盘
    file.sync_all().map_err(err)?;
    // 关闭文件
    drop(file);
    // 原子改名到目标位置
    fs::rename(&temp, path).map_err(err)?;
    // 完成
    Ok(())
}

/// 桌面后端按 URI 解码路径；移动后端直接去掉 `file://` 前缀，不解码。
// 桌面后端按 URI 解码路径；移动后端直接去掉 file:// 前缀，不解码
pub(super) fn file_url(path: &Path) -> String {
    // 路径文本
    let raw = path.to_string_lossy();
    // 移动端直接拼接
    if cfg!(any(target_os = "android", target_os = "ios")) {
        // Unix 路径直接加协议头
        return format!("file://{raw}");
    }
    // 其余平台对非安全字符做百分号编码
    let encoded: String = raw
        // 逐字节
        .bytes()
        // 映射
        .map(|b| {
            // 安全字符保持原样
            if b.is_ascii_alphanumeric() || b"/:-_.~".contains(&b) {
                // 转为字符
                (b as char).to_string()
            // 否则分支
            } else {
                // 其余按百分号转义
                format!("%{b:02X}")
            }
        })
        // 收集为列表
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
