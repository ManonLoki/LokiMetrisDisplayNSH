//! 为 WatchConnectivity 提供带版本的公开快照,以及不可变且经过检查的资源副本。
use loki_metris_pairing::assets::AssetDescriptor;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::Path,
};

// 清单文件最大 48 KiB
const MAX_MANIFEST: u64 = 48 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
// 发给手表的清单：带版本的公开快照与资源描述
pub(crate) struct Manifest {
    pub version: u32,
    // 数据流 ID，本机生成
    pub stream_id: String,
    // 修订号（十进制文本）
    pub revision: String,
    pub source_id: Option<String>,
    pub received_at_ms: Option<u64>,
    pub snapshot: Option<Value>,
    pub assets: Vec<AssetDescriptor>,
}

fn failure(_: impl std::fmt::Display) -> String {
    // 提示手表展示缓存不可用
    "手表展示缓存不可用".into()
}

/// 流 id 由本地通过 `Uuid::new_v4` 生成;来源 id 是已配对的桌面 id。
// 数据流 ID 由本机用 UUID v4 生成，必须是规范写法
fn valid_stream_id(value: &str) -> bool {
    // 标识必须是规范写法的 UUID
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}

// 重新投影快照，并去掉本地图片地址（手表不能使用）
fn project(value: &Value) -> Result<Value, String> {
    let mut snapshot =
        crate::features::connection::snapshot::project_snapshot(value, &serde_json::json!({}))
            .ok_or_else(|| failure("snapshot"))?;
    // 有 Hook 列表时
    if let Some(hooks) = snapshot["hooks"].as_array_mut() {
        for hook in hooks {
            if let Some(object) = hook.as_object_mut() {
                // 去掉图片来源字段（手表只用哈希）
                object.remove("imageSource");
            }
        }
    }
    Ok(snapshot)
}

impl Manifest {
    // 校验清单：版本、流 ID、修订号、资源数量，以及快照与资源的一致性
    pub(super) fn validate(&self) -> Result<(), String> {
        // 修订号必须是数字
        let revision = self.revision.parse::<u64>().map_err(failure)?;
        if self.version != 1
            || !valid_stream_id(&self.stream_id)
            || revision == 0
            // 或者修订号写法不规范
            || revision.to_string() != self.revision
            // 或者资源超过 12 个
            || self.assets.len() > 12
        {
            return Err(failure("manifest"));
        }
        // 要么全空（清空状态且没有资源），要么全有且快照已是规范投影
        match (&self.source_id, &self.snapshot, self.received_at_ms) {
            // 来源、时间、快照都没有且没有资源：合法
            (None, None, None) if self.assets.is_empty() => return Ok(()),
            // 来源、快照、接收时间都有
            (Some(source), Some(snapshot), Some(received))
                // 来源是合法设备标识
                if crate::features::connection::identity::valid_device_id(source)
                    && received > 0
                    && received <= 9_007_199_254_740_991
                    && project(snapshot).as_ref() == Ok(snapshot) => {}
            _ => return Err(failure("snapshot")),
        }
        // 快照引用的图片摘要
        let referenced: BTreeSet<_> = self
            .snapshot
            .as_ref()
            // 取 Hook 数组
            .and_then(|snapshot| snapshot["hooks"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|hook| hook["image"].as_str())
            .collect();
        let mut hashes = BTreeSet::new();
        // 每个资源必须有效、被引用且不重复
        for asset in &self.assets {
            asset.validate().map_err(failure)?;
            // 未被引用或哈希重复
            if !referenced.contains(asset.hash.as_str()) || !hashes.insert(asset.hash.as_str()) {
                return Err(failure("assets"));
            }
        }
        // 引用的图片必须全部有资源描述
        if hashes != referenced {
            return Err(failure("missing asset"));
        }
        Ok(())
    }
}

// 读取有界的普通文件
fn read_file(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    // 读取元数据，不跟随链接
    let meta = fs::symlink_metadata(path).map_err(failure)?;
    // 不是普通文件、是链接或超过上限
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > maximum {
        return Err(failure("file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(failure)?
        // 最多读上限加一字节
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(failure)?;
    if bytes.len() as u64 > maximum {
        return Err(failure("length"));
    }
    Ok(bytes)
}

pub(crate) fn read_manifest(root: &Path) -> Result<Manifest, String> {
    let manifest: Manifest =
        // 读取并解析清单文件
        serde_json::from_slice(&read_file(&root.join("manifest.json"), MAX_MANIFEST)?)
            .map_err(failure)?;
    manifest.validate()?;
    Ok(manifest)
}

// 创建私有目录并设置为仅所有者可访问
fn directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(failure)?;
    // 读取元数据，不跟随链接
    let meta = fs::symlink_metadata(path).map_err(failure)?;
    // 必须是真目录而非链接
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(failure("directory"));
    }
    #[cfg(unix)]
    {
        // Unix 权限扩展
        use std::os::unix::fs::PermissionsExt;
        // 目录权限设为仅所有者可访问
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(failure)?;
    }
    Ok(())
}

// 原子写入：随机后缀的临时文件，写完同步后改名
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    // 原子改名到目标位置
    let temp = path.with_extension(format!("{}.pending", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        // 可写且必须新建
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            // Unix 打开选项扩展
            use std::os::unix::fs::OpenOptionsExt;
            // 文件权限设为仅所有者可读写
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(failure)?;
        file.write_all(bytes).map_err(failure)?;
        file.sync_all().map_err(failure)?;
        drop(file);
        // 原子改名到目标位置
        fs::rename(&temp, path).map_err(failure)
    })();
    // 失败则清理临时文件
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

/// 返回清单或某个可用的、被引用的资源是否发生了变化。
// 更新发件箱：返回清单或某个被引用的资源是否发生变化
pub(super) fn update(
    root: &Path,
    link_root: &Path,
    source: Option<&str>,
    raw: Option<&Value>,
    received: Option<u64>,
) -> Result<bool, String> {
    directory(root)?;
    // 读取旧清单（若有）
    let previous = if root.join("manifest.json").exists() {
        Some(read_manifest(root)?)
    // 否则分支
    } else {
        None
    };
    // 先构造一个空状态的清单，沿用旧的流 ID 与修订号
    let mut manifest = Manifest {
        version: 1,
        // 沿用旧流标识，没有则新建
        stream_id: previous.as_ref().map_or_else(
            || uuid::Uuid::new_v4().to_string(),
            |old| old.stream_id.clone(),
        ),
        revision: previous
            .as_ref()
            // 沿用旧修订号，没有则为 0
            .map_or_else(|| "0".into(), |old| old.revision.clone()),
        source_id: None,
        received_at_ms: None,
        snapshot: None,
        assets: Vec::new(),
    };
    // 有快照则填入来源、时间、快照与被引用的资源
    if let (Some(source), Some(raw), Some(received)) = (source, raw, received) {
        let snapshot = project(raw)?;
        let referenced: BTreeSet<_> = snapshot["hooks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|hook| hook["image"].as_str())
            .collect();
        // 解析资源描述列表，失败转为错误
        let descriptors: Vec<AssetDescriptor> =
            serde_json::from_value(raw["assets"].clone()).map_err(failure)?;
        // 只保留被引用的资源
        manifest.assets = descriptors
            .into_iter()
            // 只保留被引用的
            .filter(|asset| referenced.contains(asset.hash.as_str()))
            .collect();
        // 按摘要排序，保证内容稳定
        manifest.assets.sort_by(|a, b| a.hash.cmp(&b.hash));
        manifest.source_id = Some(source.into());
        manifest.received_at_ms = Some(received);
        manifest.snapshot = Some(snapshot);
    }
    let content_changed = previous.as_ref() != Some(&manifest);
    if content_changed {
        let next = manifest
            .revision
            .parse::<u64>()
            .map_err(failure)?
            .checked_add(1)
            .ok_or_else(|| failure("revision exhausted"))?;
        manifest.revision = next.to_string();
    }
    manifest.validate()?;
    // 序列化清单，超出上限则报上下文过大
    let bytes = serde_json::to_vec(&manifest).map_err(failure)?;
    if bytes.len() as u64 > MAX_MANIFEST {
        // 返回上下文过大错误
        return Err(failure("context too large"));
    }
    // 资源目录，确保存在
    let assets = root.join("assets");
    directory(&assets)?;
    // 记录资源是否有变化
    let mut asset_changed = false;
    // 把缺失的资源从缓存目录复制到发件箱
    for asset in &manifest.assets {
        let destination = assets.join(&asset.hash);
        // 已有且校验通过的跳过
        if read_file(&destination, asset.length).is_ok_and(|bytes| asset.matches(&bytes)) {
            continue;
        }
        let source = link_root.join("assets").join(&asset.hash);
        // 来源校验通过才复制
        if let Ok(bytes) = read_file(&source, asset.length)
            // 并且内容与描述一致
            && asset.matches(&bytes)
        {
            atomic_write(&destination, &bytes)?;
            asset_changed = true;
        }
    }
    // 清单最后写入，保证引用的资源已就绪
    if content_changed {
        atomic_write(&root.join("manifest.json"), &bytes)?;
    }
    // 待处理的 WatchConnectivity 传输各自持有独立的不可变暂存副本。
    // 清理不再被引用的资源；待决的 WatchConnectivity 传输各自持有独立的不可变暂存副本
    for file in fs::read_dir(&assets).map_err(failure)?.flatten() {
        // 不再被引用的文件
        if !manifest
            .assets
            .iter()
            // 文件名等于某个资源哈希
            .any(|asset| file.file_name() == asset.hash.as_str())
        {
            let _ = fs::remove_file(file.path());
        }
    }
    // 内容或资源任一变化即为已变更
    Ok(content_changed || asset_changed)
}

#[cfg(test)]
#[path = "watch_outbox_test.rs"]
mod watch_outbox_test;
