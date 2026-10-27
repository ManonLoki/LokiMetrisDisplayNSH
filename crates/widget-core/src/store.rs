//! App 与系统扩展共享的最近摘要和独立读取凭据。跨进程文件锁让撤销、续租与在途
//! HTTP 结果按顺序提交：迟到的结果不能覆盖更新的认证摘要，也不能恢复已撤销的来源。
use crate::{summary, widget_http};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;
use serde_json::json;
use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};

const SUMMARY: &str = "widget-summary-v1.json";
const TARGET: &str = "widget-target-v1.json";
const LOCK: &str = "widget.lock";
/// 原生实现留下的旧格式；新格式写入后不再读取。
const LEGACY: [&str; 4] = [
    "display-widget-v1.json",
    "display-phone-v1.json",
    "display-widget-refresh-v1.json",
    "display-widget.lock",
];

/// 平台密钥库对读取凭据的静态加密（Android Keystore 包裹的密钥）；未设置时按明文写入
/// 应用私有或 App Group 目录。
pub trait Sealer: Send + Sync {
    fn seal(&self, plain: &[u8]) -> Option<Vec<u8>>;
    fn open(&self, sealed: &[u8]) -> Option<Vec<u8>>;
}

static SEALER: OnceLock<Box<dyn Sealer>> = OnceLock::new();

/// 进程内设置一次；之后写入的凭据都被加密，读取时拒绝明文。
pub fn set_sealer(sealer: Box<dyn Sealer>) {
    let _ = SEALER.set(sealer);
}

pub struct Store {
    directory: PathBuf,
}

impl Store {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// 读取通过完整校验的摘要，不判断时效。
    pub fn load(&self) -> Result<Option<Value>, String> {
        self.locked(|| Ok(self.read_summary()))
    }

    /// 离线时继续显示真实的最近同步；未来时间或伪造租约返回空。
    pub fn load_cached(&self, now_ms: i64) -> Result<Option<Value>, String> {
        Ok(self
            .load()?
            .filter(|record| summary::is_cached(record, now_ms)))
    }

    pub fn load_target(&self) -> Result<Option<Value>, String> {
        self.locked(|| Ok(self.read_target()))
    }

    /// 认证连接的新摘要与读取凭据在同一锁内提交；同一连接重复授予保留原代次。
    pub fn save_authenticated(
        &self,
        record: &Value,
        target: Option<&Value>,
    ) -> Result<bool, String> {
        if !summary::validate(record)
            || target.is_some_and(|value| {
                !widget_http::valid_target(value) || value["sourceId"] != record["sourceId"]
            })
        {
            return Err("小组件摘要无效".into());
        }
        self.locked(|| {
            let previous = self.read_target();
            let next = match (target, &previous) {
                (Some(target), Some(previous))
                    if widget_http::same_connection(previous, target) =>
                {
                    Some(previous.clone())
                }
                (target, _) => target.cloned(),
            };
            if next != previous {
                match &next {
                    Some(value) => self.write_target(value)?,
                    None => self.remove(TARGET)?,
                }
            }
            let accepted = self
                .read_summary()
                .as_ref()
                .and_then(summary::stamp)
                .is_none_or(|(source, received)| {
                    Some(source) != record["sourceId"].as_str()
                        || record["receivedAtMs"].as_i64() >= Some(received)
                });
            if accepted {
                self.write(SUMMARY, record)?;
                self.remove_legacy();
            }
            Ok(accepted)
        })
    }

    /// 心跳只续当前来源仍在租约内的摘要，过期摘要不能被重新标为在线。
    pub fn renew(&self, source_id: &str, now_ms: i64) -> Result<bool, String> {
        self.locked(|| {
            let Some(current) = self
                .read_summary()
                .filter(|record| record["sourceId"] == source_id)
            else {
                return Ok(false);
            };
            match summary::renewed(&current, now_ms) {
                Some(next) => self.write(SUMMARY, &next).map(|_| true),
                None => Ok(false),
            }
        })
    }

    /// 凭据已换代或请求期间收到更新摘要时，HTTP 结果不提交。
    pub fn save_fetched(
        &self,
        record: &Value,
        target: &Value,
        started_at_ms: i64,
    ) -> Result<bool, String> {
        if !summary::validate(record) || record["sourceId"] != target["sourceId"] {
            return Err("小组件摘要无效".into());
        }
        self.locked(|| {
            if self.read_target().as_ref() != Some(target) {
                return Ok(false);
            }
            let current = self.read_summary();
            let accepted =
                current
                    .as_ref()
                    .and_then(summary::stamp)
                    .is_none_or(|(source, received)| {
                        Some(source) == record["sourceId"].as_str()
                            && record["receivedAtMs"].as_i64() >= Some(received)
                            && received <= started_at_ms
                    });
            if accepted {
                self.write(SUMMARY, record)?;
            }
            Ok(accepted)
        })
    }

    /// 明确选择另一来源时撤销旧来源的摘要与凭据。
    pub fn select(&self, source_id: &str) -> Result<(), String> {
        if !summary::valid_id(source_id) {
            return Err("来源无效".into());
        }
        self.drop_where(|source| source != source_id)
    }

    /// 只撤销被忘记的来源，保留另一桌面的最近同步。
    pub fn forget(&self, source_id: &str) -> Result<(), String> {
        if !summary::valid_id(source_id) {
            return Err("来源无效".into());
        }
        self.drop_where(|source| source == source_id)
    }

    pub fn clear(&self) -> Result<(), String> {
        self.locked(|| {
            self.remove(SUMMARY)?;
            self.remove(TARGET)?;
            self.remove_legacy();
            Ok(())
        })
    }

    fn drop_where(&self, matches: impl Fn(&str) -> bool) -> Result<(), String> {
        self.locked(|| {
            let source = |value: Option<Value>| {
                value.and_then(|value| value["sourceId"].as_str().map(str::to_owned))
            };
            if self.exists(TARGET) && source(self.read_target()).is_none_or(|id| matches(&id)) {
                self.remove(TARGET)?;
            }
            if self.exists(SUMMARY) && source(self.read_summary()).is_none_or(|id| matches(&id)) {
                self.remove(SUMMARY)?;
            }
            Ok(())
        })
    }

    fn read_summary(&self) -> Option<Value> {
        self.read(SUMMARY, summary::MAX_BYTES)
            .filter(summary::validate)
    }

    fn read_target(&self) -> Option<Value> {
        let stored = self.read(TARGET, 4_096)?;
        let value = match SEALER.get() {
            Some(sealer) => {
                let sealed = STANDARD.decode(stored["sealed"].as_str()?).ok()?;
                serde_json::from_slice(&sealer.open(&sealed)?).ok()?
            }
            None => stored,
        };
        Some(value).filter(widget_http::valid_target)
    }

    fn write_target(&self, value: &Value) -> Result<(), String> {
        match SEALER.get() {
            Some(sealer) => {
                let plain = serde_json::to_vec(value).map_err(|error| error.to_string())?;
                let sealed = sealer.seal(&plain).ok_or("读取凭据加密失败")?;
                self.write(TARGET, &json!({ "sealed": STANDARD.encode(sealed) }))
            }
            None => self.write(TARGET, value),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }

    fn exists(&self, name: &str) -> bool {
        fs::symlink_metadata(self.path(name)).is_ok()
    }

    /// 只读普通文件，拒绝符号链接与超限内容。
    fn read(&self, name: &str, limit: usize) -> Option<Value> {
        let path = self.path(name);
        let metadata = fs::symlink_metadata(&path).ok()?;
        if !metadata.is_file() || metadata.len() > limit as u64 {
            return None;
        }
        let mut bytes = Vec::new();
        File::open(&path)
            .ok()?
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        (bytes.len() <= limit)
            .then(|| serde_json::from_slice(&bytes).ok())
            .flatten()
    }

    /// 临时文件写完再原子替换，进程中断不会留下半份 JSON。
    fn write(&self, name: &str, value: &Value) -> Result<(), String> {
        let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
        let path = self.path(name);
        if fs::symlink_metadata(&path).is_ok_and(|metadata| !metadata.is_file()) {
            return Err("小组件共享文件不可用".into());
        }
        let temporary = self.path(&format!(".{name}.tmp"));
        fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        fs::rename(&temporary, &path).map_err(|error| error.to_string())
    }

    fn remove(&self, name: &str) -> Result<(), String> {
        match fs::remove_file(self.path(name)) {
            Err(error) if error.kind() != ErrorKind::NotFound => Err(error.to_string()),
            _ => Ok(()),
        }
    }

    fn remove_legacy(&self) {
        for name in LEGACY {
            let _ = self.remove(name);
        }
    }

    /// 固定锁文件不删除，避免不同进程锁住不同 inode；短时重试限制挂起进程的影响。
    fn locked<T>(&self, body: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let unavailable = || "小组件共享目录不可用".to_owned();
        if !fs::symlink_metadata(&self.directory).is_ok_and(|metadata| metadata.is_dir()) {
            return Err(unavailable());
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path(&self.directory))
            .map_err(|_| unavailable())?;
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut backoff = Duration::from_millis(5);
        while file.try_lock().is_err() {
            if Instant::now() >= deadline {
                return Err(unavailable());
            }
            thread::sleep(backoff);
            backoff = (backoff * 2).min(Duration::from_millis(100));
        }
        let result = body();
        let _ = file.unlock();
        result
    }
}

fn lock_path(directory: &Path) -> PathBuf {
    directory.join(LOCK)
}

#[cfg(test)]
#[path = "store_test.rs"]
mod store_test;
