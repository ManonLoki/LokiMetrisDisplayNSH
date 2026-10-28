//! 图片分块接收与私有磁盘缓存。
//! 只有长度和摘要都校验成功的私有缓存文件，才会转换成界面可读的 URI。
// 引入错误转换、owner 与持久化
use super::{err, owner::Owner, persistence};
// 引入资源分块与接收器
use loki_metris_pairing::assets::{AssetChunk, AssetDescriptor, AssetReceiver};
// 引入 JSON
use serde_json::{Value, json};
// 引入集合、文件系统、路径与时间
use std::{
    // 有序映射与有序集合
    collections::{BTreeMap, BTreeSet},
    // 文件系统
    fs,
    // 路径类型
    path::{Path, PathBuf},
    // 时长、时刻与纪元
    time::{Duration, Instant, UNIX_EPOCH},
};

// 单张图片 15 秒没有进展即超时
const ASSET_TIMEOUT: Duration = Duration::from_secs(15);
// 缓存总量上限 512 MiB
const CACHE_BYTES: u64 = 512 * 1024 * 1024;
// 缓存文件数上限 512
const CACHE_FILES: usize = 512;

/// 当前快照的资源清单与唯一在途接收器。
#[derive(Default)]
// 当前快照的资源清单与唯一在途接收器
pub(super) struct AssetState {
    // 资源清单，以摘要索引
    descriptors: BTreeMap<String, AssetDescriptor>,
    // 在途接收器
    receiver: Option<(AssetDescriptor, AssetReceiver)>,
    // 最近一次进展时间
    progress: Option<Instant>,
    // 已失败的摘要，本连接内不再重试
    failed: BTreeSet<String>,
}
// 资源状态的方法
impl AssetState {
    /// 换连接时撤销在途接收器与失败记录。
    // 换连接时撤销在途接收器与失败记录
    pub(super) fn reset(&mut self) {
        // 清除接收器
        self.receiver = None;
        // 清除进度
        self.progress = None;
        // 清空失败记录
        self.failed.clear();
    }
    /// 新快照替换清单；在途资源不再被引用时放弃它。
    // 新快照替换清单；在途资源不再被引用时放弃它
    pub(super) fn replace(&mut self, descriptors: BTreeMap<String, AssetDescriptor>) {
        // 保存新的资源描述
        self.descriptors = descriptors;
        // 接收器对应的资源已不在新清单里
        if self
            // 正在接收的资源
            .receiver
            // 取引用
            .as_ref()
            // 与清单中的描述不一致
            .is_some_and(|r| self.descriptors.get(&r.0.hash) != Some(&r.0))
        {
            // 清除接收器
            self.receiver = None;
            // 清除进度
            self.progress = None;
        }
    }
    // 清单中是否有该摘要
    pub(super) fn contains(&self, hash: &str) -> bool {
        // 清单里是否有该哈希
        self.descriptors.contains_key(hash)
    }
    // 在途接收器的超时时刻
    pub(super) fn deadline(&self) -> Option<Instant> {
        // 正在接收的资源
        self.receiver
            // 取引用
            .as_ref()
            // 与进度配对
            .and(self.progress)
            // 期限为进度时刻加超时
            .map(|at| at + ASSET_TIMEOUT)
    }
}

// 图片缓存目录
pub(super) fn asset_dir(root: &Path) -> PathBuf {
    // 资源目录
    root.join("assets")
}

// 所有者的方法
impl Owner {
    /// 按 Hook 顺序取下一张缺失的引用图片；后台不取图片。
    // 按 Hook 顺序取下一张缺失的引用图片；后台不取图片
    pub(super) fn next_asset(&mut self) -> Result<(), String> {
        // 已有在途接收器或不可见则不处理
        if self.assets.receiver.is_some() || !self.lifecycle.visible {
            // 没有可处理的内容，直接返回
            return Ok(());
        }
        // 快照里的 Hook 列表
        let hooks = self.view["snapshot"]["hooks"]
            // 转为数组
            .as_array()
            // 复制
            .cloned()
            // 缺省为空
            .unwrap_or_default();
        // 逐条 Hook 找出需要的图片
        for h in hooks {
            // 没有图片的跳过
            let Some(hash) = h["image"].as_str() else {
                // 跳过
                continue;
            };
            // 已失败或已就绪的跳过
            if self.assets.failed.contains(hash) || self.view["images"][hash].is_string() {
                // 跳过
                continue;
            }
            // 引用必须在清单中
            let Some(d) = self.assets.descriptors.get(hash).cloned() else {
                // 图片引用不在清单中
                return Err("图片引用不在清单中".into());
            };
            // 资源文件路径
            let path = asset_dir(&self.root).join(hash);
            // 本地缓存已有且校验通过：直接使用
            if read_asset(&path, &d).is_ok() {
                // 已缓存：记录图片地址
                self.view["images"][hash] = json!(persistence::file_url(&path));
                // 处理下一个
                continue;
            }
            // 否则创建接收器并请求第一块
            let receiver = AssetReceiver::new(d.clone()).map_err(err)?;
            // 开始接收该资源
            self.assets.receiver = Some((d, receiver));
            // 记录进度时刻
            self.assets.progress = Some(Instant::now());
            // 请求该资源的第一块
            self.send_secure(json!({"type":"assetRequest","hash":hash,"offset":0}))?;
            // 一次只收一张
            break;
        }
        // 完成
        Ok(())
    }

    // 处理图片分块
    pub(super) fn asset(&mut self, v: Value) -> Result<(), String> {
        // 解析资源分块
        let chunk: AssetChunk = serde_json::from_value(v).map_err(err)?;
        // 必须有在途接收器
        let (d, receiver) = self.assets.receiver.as_mut().ok_or("未请求的图片块")?;
        // 校验并接收分块
        let complete = receiver.accept(&chunk).map_err(err)?;
        // 记录进展
        self.assets.progress = Some(Instant::now());
        // 当前接收偏移
        let offset = receiver.offset();
        // 资源哈希
        let hash = d.hash.clone();
        // 未完成：请求下一块
        if !complete {
            // 请求下一块
            return self.send_secure(json!({"type":"assetRequest","hash":hash,"offset":offset}));
        }
        // 完成：取出完整字节
        let (d, bytes) = self
            // 资源状态
            .assets
            // 接收器
            .receiver
            // 取走
            .take()
            // 没有则报无效图片状态
            .ok_or("无效图片状态")?
            // 取描述之后的接收器
            .1
            // 结束接收
            .finish()
            // 失败转为错误
            .map_err(err)?;
        // 清除进度
        self.assets.progress = None;
        // 资源目录
        let dir = asset_dir(&self.root);
        // 资源文件路径
        let path = dir.join(&d.hash);
        // 落盘并再次校验
        let stored = persistence::private_dir(&dir)
            // 写入缓存文件
            .and_then(|_| persistence::atomic_write(&path, &bytes))
            // 并校验
            .and_then(|_| read_asset(&path, &d));
        // 成功：登记图片地址
        if stored.is_ok() {
            // 记录图片地址
            self.view["images"][&d.hash] = json!(persistence::file_url(&path));
        // 失败：记录并提示，文字与用量仍可显示
        } else {
            // 标记该资源失败
            self.assets.failed.insert(d.hash);
            // 提示图片缓存保存失败
            self.view["assetWarning"] = json!("图片缓存保存失败，文字与用量仍可显示。");
        }
        // 控制缓存规模
        self.prune_assets();
        // 继续下一张
        self.next_asset()?;
        // 发布视图
        self.publish();
        // 完成
        Ok(())
    }

    // 桌面表示图片不可用：必须对应在途接收器
    pub(super) fn asset_unavailable(&mut self, hash: &str) -> Result<(), String> {
        // 响应与当前接收不匹配
        if self
            // 资源状态
            .assets
            // 接收器
            .receiver
            // 取引用
            .as_ref()
            // 没有或哈希不一致
            .is_none_or(|r| r.0.hash != hash)
        {
            // 无效图片响应
            return Err("无效图片响应".into());
        }
        // 记为失败并继续下一张
        self.assets.failed.insert(hash.into());
        // 清除接收器
        self.assets.receiver = None;
        // 清除进度
        self.assets.progress = None;
        // 继续下一个资源
        self.next_asset()
    }

    /// 单张图片超时只放弃这一张，文字与用量继续同步。
    // 单张图片超时只放弃这一张，文字与用量继续同步
    pub(super) fn asset_timeout(&mut self) {
        // 取走当前接收器
        if let Some((d, _)) = self.assets.receiver.take() {
            // 标记该资源失败
            self.assets.failed.insert(d.hash);
        }
        // 清除进度
        self.assets.progress = None;
        // 提示图片读取超时
        self.view["assetWarning"] = json!("图片读取超时，已保留文字与用量。");
        // 继续下一个资源
        if let Err(e) = self.next_asset() {
            // 失败则记录
            self.failed(e);
        }
        // 发布视图
        self.publish();
    }

    /// 缓存优先保留当前引用，其余按修改时间收敛到总量与文件数上限。
    // 缓存优先保留当前引用，其余按修改时间收敛到总量与文件数上限
    fn prune_assets(&self) {
        // 读取资源目录
        let Ok(entries) = fs::read_dir(asset_dir(&self.root)) else {
            // 失败则返回
            return;
        };
        // 当前引用的摘要
        let references: BTreeSet<_> = self.view["snapshot"]["hooks"]
            // 转为数组
            .as_array()
            // 转为迭代器
            .into_iter()
            // 展平
            .flatten()
            // 取图片哈希
            .filter_map(|h| h["image"].as_str())
            // 收集为集合
            .collect();
        // 文件列表
        let mut files = Vec::new();
        // 总大小
        let mut total = 0u64;
        // 只统计 64 位十六进制命名的普通文件
        for entry in entries.flatten() {
            // 文件名
            let name = entry.file_name().to_string_lossy().into_owned();
            // 名称长度须为 64
            if name.len() != 64
                // 或者
                || !name
                    // 文件名按字节
                    .bytes()
                    // 须为小写十六进制
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                // 不合格则跳过
                continue;
            }
            // 取不到元数据则跳过
            let Ok(meta) = entry.metadata() else { continue };
            // 不是文件则跳过
            if !meta.is_file() {
                // 跳过
                continue;
            }
            // 累加总大小
            total = total.saturating_add(meta.len());
            // 记录文件信息
            files.push((
                // 修改时间
                meta.modified().unwrap_or(UNIX_EPOCH),
                // 路径
                entry.path(),
                // 大小
                meta.len(),
                // 是否仍被引用
                references.contains(name.as_str()),
            ));
        }
        // 按修改时间从旧到新
        files.sort_by_key(|f| f.0);
        // 文件数
        let mut count = files.len();
        // 超限时从最旧的开始删除，保留当前引用的
        for (_, path, length, keep) in files {
            // 总大小与数量都在限额内
            if total <= CACHE_BYTES && count <= CACHE_FILES {
                // 结束清理
                break;
            }
            // 没有被引用才删除
            if !keep && fs::remove_file(path).is_ok() {
                // 扣减总大小
                total = total.saturating_sub(length);
                // 文件数减一
                count -= 1;
            }
        }
    }

    /// 离线恢复的快照只展示本地已校验的图片，不发起任何请求。
    // 离线恢复的快照只展示本地已校验的图片，不发起任何请求
    pub(super) fn restore_cached_images(&mut self) {
        // 解析资源描述
        let Ok(descriptors) =
            // 解析快照里的资源描述
            serde_json::from_value::<Vec<AssetDescriptor>>(self.view["snapshot"]["assets"].clone())
        // 失败则忽略
        else {
            // 失败则返回
            return;
        };
        // 逐个资源
        for d in descriptors {
            // 资源文件路径
            let path = asset_dir(&self.root).join(&d.hash);
            // 读取并校验通过
            if read_asset(&path, &d).is_ok() {
                // 记录图片地址
                self.view["images"][&d.hash] = json!(persistence::file_url(&path));
            }
        }
    }
}

// 通过文件校验器检查缓存文件
pub(super) fn read_asset(path: &Path, d: &AssetDescriptor) -> Result<(), String> {
    // 校验文件内容
    super::asset_cache::verify_file(path, d)
}
