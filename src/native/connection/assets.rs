//! 图片分块接收与私有磁盘缓存。
//! 只有长度和摘要都校验成功的私有缓存文件，才会转换成界面可读的 URI。
use super::{err, owner::Owner, persistence};
use loki_metris_pairing::assets::{AssetChunk, AssetDescriptor, AssetReceiver};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, UNIX_EPOCH},
};

const ASSET_TIMEOUT: Duration = Duration::from_secs(15);
const CACHE_BYTES: u64 = 512 * 1024 * 1024;
const CACHE_FILES: usize = 512;

/// 当前快照的资源清单与唯一在途接收器。
#[derive(Default)]
pub(super) struct AssetState {
    descriptors: BTreeMap<String, AssetDescriptor>,
    receiver: Option<(AssetDescriptor, AssetReceiver)>,
    progress: Option<Instant>,
    failed: BTreeSet<String>,
}
impl AssetState {
    /// 换连接时撤销在途接收器与失败记录。
    pub(super) fn reset(&mut self) {
        self.receiver = None;
        self.progress = None;
        self.failed.clear();
    }
    /// 新快照替换清单；在途资源不再被引用时放弃它。
    pub(super) fn replace(&mut self, descriptors: BTreeMap<String, AssetDescriptor>) {
        self.descriptors = descriptors;
        if self
            .receiver
            .as_ref()
            .is_some_and(|r| self.descriptors.get(&r.0.hash) != Some(&r.0))
        {
            self.receiver = None;
            self.progress = None;
        }
    }
    pub(super) fn contains(&self, hash: &str) -> bool {
        self.descriptors.contains_key(hash)
    }
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.receiver
            .as_ref()
            .and(self.progress)
            .map(|at| at + ASSET_TIMEOUT)
    }
}

pub(super) fn asset_dir(root: &Path) -> PathBuf {
    root.join("assets")
}

impl Owner {
    /// 按 Hook 顺序取下一张缺失的引用图片；后台不取图片。
    pub(super) fn next_asset(&mut self) -> Result<(), String> {
        if self.assets.receiver.is_some() || !self.lifecycle.visible {
            return Ok(());
        }
        let hooks = self.view["snapshot"]["hooks"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for h in hooks {
            let Some(hash) = h["image"].as_str() else {
                continue;
            };
            if self.assets.failed.contains(hash) || self.view["images"][hash].is_string() {
                continue;
            }
            let Some(d) = self.assets.descriptors.get(hash).cloned() else {
                return Err("图片引用不在清单中".into());
            };
            let path = asset_dir(&self.root).join(hash);
            if read_asset(&path, &d).is_ok() {
                self.view["images"][hash] = json!(persistence::file_url(&path));
                continue;
            }
            let receiver = AssetReceiver::new(d.clone()).map_err(err)?;
            self.assets.receiver = Some((d, receiver));
            self.assets.progress = Some(Instant::now());
            self.send_secure(json!({"type":"assetRequest","hash":hash,"offset":0}))?;
            break;
        }
        Ok(())
    }

    pub(super) fn asset(&mut self, v: Value) -> Result<(), String> {
        let chunk: AssetChunk = serde_json::from_value(v).map_err(err)?;
        let (d, receiver) = self.assets.receiver.as_mut().ok_or("未请求的图片块")?;
        let complete = receiver.accept(&chunk).map_err(err)?;
        self.assets.progress = Some(Instant::now());
        let offset = receiver.offset();
        let hash = d.hash.clone();
        if !complete {
            return self.send_secure(json!({"type":"assetRequest","hash":hash,"offset":offset}));
        }
        let (d, bytes) = self
            .assets
            .receiver
            .take()
            .ok_or("无效图片状态")?
            .1
            .finish()
            .map_err(err)?;
        self.assets.progress = None;
        let dir = asset_dir(&self.root);
        let path = dir.join(&d.hash);
        let stored = persistence::private_dir(&dir)
            .and_then(|_| persistence::atomic_write(&path, &bytes))
            .and_then(|_| read_asset(&path, &d));
        if stored.is_ok() {
            self.view["images"][&d.hash] = json!(persistence::file_url(&path));
        } else {
            self.assets.failed.insert(d.hash);
            self.view["assetWarning"] = json!("图片缓存保存失败，文字与用量仍可显示。");
        }
        self.prune_assets();
        self.next_asset()?;
        self.publish();
        Ok(())
    }

    pub(super) fn asset_unavailable(&mut self, hash: &str) -> Result<(), String> {
        if self
            .assets
            .receiver
            .as_ref()
            .is_none_or(|r| r.0.hash != hash)
        {
            return Err("无效图片响应".into());
        }
        self.assets.failed.insert(hash.into());
        self.assets.receiver = None;
        self.assets.progress = None;
        self.next_asset()
    }

    /// 单张图片超时只放弃这一张，文字与用量继续同步。
    pub(super) fn asset_timeout(&mut self) {
        if let Some((d, _)) = self.assets.receiver.take() {
            self.assets.failed.insert(d.hash);
        }
        self.assets.progress = None;
        self.view["assetWarning"] = json!("图片读取超时，已保留文字与用量。");
        if let Err(e) = self.next_asset() {
            self.failed(e);
        }
        self.publish();
    }

    /// 缓存优先保留当前引用，其余按修改时间收敛到总量与文件数上限。
    fn prune_assets(&self) {
        let Ok(entries) = fs::read_dir(asset_dir(&self.root)) else {
            return;
        };
        let references: BTreeSet<_> = self.view["snapshot"]["hooks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|h| h["image"].as_str())
            .collect();
        let mut files = Vec::new();
        let mut total = 0u64;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.len() != 64
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            total = total.saturating_add(meta.len());
            files.push((
                meta.modified().unwrap_or(UNIX_EPOCH),
                entry.path(),
                meta.len(),
                references.contains(name.as_str()),
            ));
        }
        files.sort_by_key(|f| f.0);
        let mut count = files.len();
        for (_, path, length, keep) in files {
            if total <= CACHE_BYTES && count <= CACHE_FILES {
                break;
            }
            if !keep && fs::remove_file(path).is_ok() {
                total = total.saturating_sub(length);
                count -= 1;
            }
        }
    }

    /// 离线恢复的快照只展示本地已校验的图片，不发起任何请求。
    pub(super) fn restore_cached_images(&mut self) {
        let Ok(descriptors) =
            serde_json::from_value::<Vec<AssetDescriptor>>(self.view["snapshot"]["assets"].clone())
        else {
            return;
        };
        for d in descriptors {
            let path = asset_dir(&self.root).join(&d.hash);
            if read_asset(&path, &d).is_ok() {
                self.view["images"][&d.hash] = json!(persistence::file_url(&path));
            }
        }
    }
}

pub(super) fn read_asset(path: &Path, d: &AssetDescriptor) -> Result<(), String> {
    super::asset_cache::verify_file(path, d)
}
