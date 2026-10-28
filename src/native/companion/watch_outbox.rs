//! 为 WatchConnectivity 提供带版本的公开快照,以及不可变且经过检查的资源副本。
// 引入资源描述
use loki_metris_pairing::assets::AssetDescriptor;
// 引入序列化
use serde::{Deserialize, Serialize};
// 引入 JSON
use serde_json::Value;
// 引入集合、文件系统、读写与路径
use std::{
    // 有序集合
    collections::BTreeSet,
    // 文件系统
    fs,
    // 读写特征
    io::{Read, Write},
    // 路径类型
    path::Path,
};

// 清单文件最大 48 KiB
const MAX_MANIFEST: u64 = 48 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
// 发给手表的清单：带版本的公开快照与资源描述
pub(crate) struct Manifest {
    // 清单版本
    pub version: u32,
    // 数据流 ID，本机生成
    pub stream_id: String,
    // 修订号（十进制文本）
    pub revision: String,
    // 来源桌面 ID
    pub source_id: Option<String>,
    // 快照接收时间
    pub received_at_ms: Option<u64>,
    // 公开快照
    pub snapshot: Option<Value>,
    // 引用的图片资源
    pub assets: Vec<AssetDescriptor>,
}

// 统一的失败文案
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
    // 复制快照以便修改
    let mut snapshot =
        // 投影出展示快照
        crate::features::connection::snapshot::project_snapshot(value, &serde_json::json!({}))
            // 失败报快照无效
            .ok_or_else(|| failure("snapshot"))?;
    // 有 Hook 列表时
    if let Some(hooks) = snapshot["hooks"].as_array_mut() {
        // 逐个 Hook
        for hook in hooks {
            // 取其对象形式
            if let Some(object) = hook.as_object_mut() {
                // 去掉图片来源字段（手表只用哈希）
                object.remove("imageSource");
            }
        }
    }
    // 返回快照
    Ok(snapshot)
}

// 清单的方法
impl Manifest {
    // 校验清单：版本、流 ID、修订号、资源数量，以及快照与资源的一致性
    pub(super) fn validate(&self) -> Result<(), String> {
        // 修订号必须是数字
        let revision = self.revision.parse::<u64>().map_err(failure)?;
        // 版本须为 1
        if self.version != 1
            // 或者流标识无效
            || !valid_stream_id(&self.stream_id)
            // 或者修订号为 0
            || revision == 0
            // 或者修订号写法不规范
            || revision.to_string() != self.revision
            // 或者资源超过 12 个
            || self.assets.len() > 12
        {
            // 清单无效
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
                    // 接收时间为正
                    && received > 0
                    // 不超过安全整数
                    && received <= 9_007_199_254_740_991
                    // 快照已是规范投影
                    && project(snapshot).as_ref() == Ok(snapshot) => {}
            // 其他组合都无效
            _ => return Err(failure("snapshot")),
        }
        // 快照引用的图片摘要
        let referenced: BTreeSet<_> = self
            // 快照
            .snapshot
            // 取引用
            .as_ref()
            // 取 Hook 数组
            .and_then(|snapshot| snapshot["hooks"].as_array())
            // 转为迭代器
            .into_iter()
            // 展平
            .flatten()
            // 取图片哈希
            .filter_map(|hook| hook["image"].as_str())
            // 收集为集合
            .collect();
        // 已见哈希集合
        let mut hashes = BTreeSet::new();
        // 每个资源必须有效、被引用且不重复
        for asset in &self.assets {
            // 校验资源描述
            asset.validate().map_err(failure)?;
            // 未被引用或哈希重复
            if !referenced.contains(asset.hash.as_str()) || !hashes.insert(asset.hash.as_str()) {
                // 资源列表无效
                return Err(failure("assets"));
            }
        }
        // 引用的图片必须全部有资源描述
        if hashes != referenced {
            // 有引用的资源缺失
            return Err(failure("missing asset"));
        }
        // 完成
        Ok(())
    }
}

// 读取有界的普通文件
fn read_file(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    // 读取元数据，不跟随链接
    let meta = fs::symlink_metadata(path).map_err(failure)?;
    // 不是普通文件、是链接或超过上限
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > maximum {
        // 文件无效
        return Err(failure("file"));
    }
    // 读取缓冲区
    let mut bytes = Vec::new();
    // 打开文件
    fs::File::open(path)
        // 失败转为错误
        .map_err(failure)?
        // 最多读上限加一字节
        .take(maximum + 1)
        // 读入缓冲区
        .read_to_end(&mut bytes)
        // 失败转为错误
        .map_err(failure)?;
    // 超出上限
    if bytes.len() as u64 > maximum {
        // 长度无效
        return Err(failure("length"));
    }
    // 返回字节
    Ok(bytes)
}

// 读取并校验清单
pub(crate) fn read_manifest(root: &Path) -> Result<Manifest, String> {
    // 解析清单
    let manifest: Manifest =
        // 读取并解析清单文件
        serde_json::from_slice(&read_file(&root.join("manifest.json"), MAX_MANIFEST)?)
            // 失败转为错误
            .map_err(failure)?;
    // 校验清单
    manifest.validate()?;
    // 返回清单
    Ok(manifest)
}

// 创建私有目录并设置为仅所有者可访问
fn directory(path: &Path) -> Result<(), String> {
    // 创建目录
    fs::create_dir_all(path).map_err(failure)?;
    // 读取元数据，不跟随链接
    let meta = fs::symlink_metadata(path).map_err(failure)?;
    // 必须是真目录而非链接
    if !meta.is_dir() || meta.file_type().is_symlink() {
        // 目录无效
        return Err(failure("directory"));
    }
    #[cfg(unix)]
    {
        // Unix 权限扩展
        use std::os::unix::fs::PermissionsExt;
        // 目录权限设为仅所有者可访问
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(failure)?;
    }
    // 完成
    Ok(())
}

// 刷盘
// 原子写入：随机后缀的临时文件，写完同步后改名
// 关闭文件
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    // 原子改名到目标位置
    let temp = path.with_extension(format!("{}.pending", uuid::Uuid::new_v4()));
    // 在闭包里写入
    let result = (|| {
        // 打开选项
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
        // 打开临时文件
        let mut file = options.open(&temp).map_err(failure)?;
        // 写入全部字节
        file.write_all(bytes).map_err(failure)?;
        // 刷盘
        file.sync_all().map_err(failure)?;
        // 关闭文件
        drop(file);
        // 原子改名到目标位置
        fs::rename(&temp, path).map_err(failure)
        // 闭包结束
    })();
    // 失败则清理临时文件
    if result.is_err() {
        // 清理临时文件
        let _ = fs::remove_file(temp);
    }
    // 返回结果
    result
}

/// 返回清单或某个可用的、被引用的资源是否发生了变化。
// 更新发件箱：返回清单或某个被引用的资源是否发生变化
pub(super) fn update(
    // 根目录
    root: &Path,
    // 链接根目录
    link_root: &Path,
    // 来源
    source: Option<&str>,
    // 原始数据
    raw: Option<&Value>,
    // 接收时间
    received: Option<u64>,
    // 返回是否有变化
) -> Result<bool, String> {
    // 确保目录存在
    directory(root)?;
    // 读取旧清单（若有）
    let previous = if root.join("manifest.json").exists() {
        // 读取旧清单
        Some(read_manifest(root)?)
    // 否则分支
    } else {
        // 没有旧清单
        None
    };
    // 先构造一个空状态的清单，沿用旧的流 ID 与修订号
    let mut manifest = Manifest {
        // 清单版本为 1
        version: 1,
        // 沿用旧流标识，没有则新建
        stream_id: previous.as_ref().map_or_else(
            // 新建 UUID
            || uuid::Uuid::new_v4().to_string(),
            // 沿用旧流标识
            |old| old.stream_id.clone(),
        ),
        // 修订号
        revision: previous
            // 取引用
            .as_ref()
            // 沿用旧修订号，没有则为 0
            .map_or_else(|| "0".into(), |old| old.revision.clone()),
        // 尚无来源
        source_id: None,
        // 尚无接收时间
        received_at_ms: None,
        // 尚无快照
        snapshot: None,
        // 尚无资源
        assets: Vec::new(),
    };
    // 有快照则填入来源、时间、快照与被引用的资源
    if let (Some(source), Some(raw), Some(received)) = (source, raw, received) {
        // 投影出展示快照
        let snapshot = project(raw)?;
        // 被引用的资源哈希
        let referenced: BTreeSet<_> = snapshot["hooks"]
            // 转为数组
            .as_array()
            // 转为迭代器
            .into_iter()
            // 展平
            .flatten()
            // 取图片哈希
            .filter_map(|hook| hook["image"].as_str())
            // 收集为集合
            .collect();
        // 解析资源描述列表，失败转为错误
        let descriptors: Vec<AssetDescriptor> =
            // 解析资源描述列表
            serde_json::from_value(raw["assets"].clone()).map_err(failure)?;
        // 只保留被引用的资源
        manifest.assets = descriptors
            // 遍历
            .into_iter()
            // 只保留被引用的
            .filter(|asset| referenced.contains(asset.hash.as_str()))
            // 收集为列表
            .collect();
        // 按摘要排序，保证内容稳定
        manifest.assets.sort_by(|a, b| a.hash.cmp(&b.hash));
        // 记录来源
        manifest.source_id = Some(source.into());
        // 记录接收时间
        manifest.received_at_ms = Some(received);
        // 记录快照
        manifest.snapshot = Some(snapshot);
    }
    // 内容是否变化
    let content_changed = previous.as_ref() != Some(&manifest);
    // 变化则修订号加一
    if content_changed {
        // 新修订号
        let next = manifest
            // 修订号
            .revision
            // 解析为数字
            .parse::<u64>()
            // 失败转为错误
            .map_err(failure)?
            // 加一，溢出则失败
            .checked_add(1)
            // 溢出报修订号耗尽
            .ok_or_else(|| failure("revision exhausted"))?;
        // 写回修订号
        manifest.revision = next.to_string();
    }
    // 校验清单
    manifest.validate()?;
    // 序列化清单，超出上限则报上下文过大
    let bytes = serde_json::to_vec(&manifest).map_err(failure)?;
    // 超出清单上限
    if bytes.len() as u64 > MAX_MANIFEST {
        // 返回上下文过大错误
        return Err(failure("context too large"));
    }
    // 资源目录，确保存在
    let assets = root.join("assets");
    // 确保资源目录存在
    directory(&assets)?;
    // 记录资源是否有变化
    let mut asset_changed = false;
    // 把缺失的资源从缓存目录复制到发件箱
    for asset in &manifest.assets {
        // 目的文件路径
        let destination = assets.join(&asset.hash);
        // 已有且校验通过的跳过
        if read_file(&destination, asset.length).is_ok_and(|bytes| asset.matches(&bytes)) {
            // 跳过
            continue;
        }
        // 来源文件路径
        let source = link_root.join("assets").join(&asset.hash);
        // 来源校验通过才复制
        if let Ok(bytes) = read_file(&source, asset.length)
            // 并且内容与描述一致
            && asset.matches(&bytes)
        {
            // 原子写入
            atomic_write(&destination, &bytes)?;
            // 标记资源有变化
            asset_changed = true;
        }
    }
    // 清单最后写入，保证引用的资源已就绪
    if content_changed {
        // 原子写入清单文件
        atomic_write(&root.join("manifest.json"), &bytes)?;
    }
    // 待处理的 WatchConnectivity 传输各自持有独立的不可变暂存副本。
    // 清理不再被引用的资源；待决的 WatchConnectivity 传输各自持有独立的不可变暂存副本
    for file in fs::read_dir(&assets).map_err(failure)?.flatten() {
        // 不再被引用的文件
        if !manifest
            // 清单资源
            .assets
            // 遍历
            .iter()
            // 文件名等于某个资源哈希
            .any(|asset| file.file_name() == asset.hash.as_str())
        {
            // 删除，失败忽略
            let _ = fs::remove_file(file.path());
        }
    }
    // 内容或资源任一变化即为已变更
    Ok(content_changed || asset_changed)
}

// 测试模块
#[cfg(test)]
#[path = "watch_outbox_test.rs"]
mod watch_outbox_test;
