//! App 与系统扩展共享的最近摘要和独立读取凭据。跨进程文件锁让撤销、续租与在途
//! HTTP 结果按顺序提交：迟到的结果不能覆盖更新的认证摘要，也不能恢复已撤销的来源。
use crate::{summary, widget_http};
use serde_json::Value;
use std::{
    // 文件操作与打开选项
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
    // 线程
    thread,
    time::{Duration, Instant},
};

const SUMMARY: &str = "widget-summary-v1.json";
const TARGET: &str = "widget-target-v1.json";
// 跨进程锁文件名
const LOCK: &str = "widget.lock";
/// 原生实现留下的旧格式；新格式写入后不再读取。
// 原生实现留下的旧格式文件；新格式写入后不再读取
const LEGACY: [&str; 4] = [
    "display-widget-v1.json",
    "display-phone-v1.json",
    "display-widget-refresh-v1.json",
    // 锁文件
    "display-widget.lock",
];

// 小组件共享存储：主应用与系统扩展共用的最近摘要和独立读取凭据
pub struct Store {
    directory: PathBuf,
}

impl Store {
    // 以共享目录创建存储
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// 读取通过完整校验的摘要，不判断时效。
    // 读取通过完整校验的摘要，不判断时效
    pub fn load(&self) -> Result<Option<Value>, String> {
        // 加锁后读取摘要
        self.locked(|| Ok(self.read_summary()))
    }

    /// 离线时继续显示真实的最近同步；未来时间或伪造租约返回空。
    // 离线时继续显示真实的最近同步；未来时间或伪造租约返回空
    pub fn load_cached(&self, now_ms: i64) -> Result<Option<Value>, String> {
        Ok(self
            .load()?
            // 只保留仍有效的缓存
            .filter(|record| summary::is_cached(record, now_ms)))
    }

    pub fn load_target(&self) -> Result<Option<Value>, String> {
        // 加锁后读取凭据
        self.locked(|| Ok(self.read_target()))
    }

    /// 认证连接的新摘要与读取凭据在同一锁内提交；同一连接重复授予保留原代次。
    // 认证连接的新摘要与读取凭据在同一把锁内提交；同一连接重复授予保留原代次
    pub fn save_authenticated(
        &self,
        record: &Value,
        target: Option<&Value>,
    ) -> Result<bool, String> {
        // 摘要或凭据无效，或两者来源不一致
        if !summary::validate(record)
            || target.is_some_and(|value| {
                // 凭据无效或与摘要来源不一致
                !widget_http::valid_target(value) || value["sourceId"] != record["sourceId"]
            })
        {
            return Err("小组件摘要无效".into());
        }
        // 加锁后执行
        self.locked(|| {
            // 先读出已有的凭据
            let previous = self.read_target();
            // 同一连接保留原凭据，否则采用新凭据
            let next = match (target, &previous) {
                // 新凭据与旧凭据都存在
                (Some(target), Some(previous))
                    if widget_http::same_connection(previous, target) =>
                {
                    Some(previous.clone())
                }
                (target, _) => target.cloned(),
            };
            if next != previous {
                // 按计算出的凭据处理
                match &next {
                    Some(value) => self.write_target(value)?,
                    None => self.remove(TARGET)?,
                }
            }
            // 同一来源的摘要不得被更旧的接收时间覆盖
            let accepted = self
                .read_summary()
                .as_ref()
                .and_then(summary::stamp)
                // 没有当前摘要，或者
                .is_none_or(|(source, received)| {
                    Some(source) != record["sourceId"].as_str()
                        // 新摘要不早于当前摘要
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
    // 心跳只续当前来源仍在租约内的摘要，过期的摘要不能被重新标为在线
    pub fn renew(&self, source_id: &str, now_ms: i64) -> Result<bool, String> {
        // 加锁后执行
        self.locked(|| {
            let Some(current) = self
                .read_summary()
                .filter(|record| record["sourceId"] == source_id)
            // 否则不处理
            else {
                // 没有当前摘要，不续租
                return Ok(false);
            };
            // 只有仍在租约内才能续租
            match summary::renewed(&current, now_ms) {
                // 续租成功则写入新截止时间
                Some(next) => self.write(SUMMARY, &next).map(|_| true),
                // 否则不续租
                None => Ok(false),
            }
        })
    }

    /// 凭据已换代或请求期间收到更新摘要时，HTTP 结果不提交。
    // 保存 HTTP 取回的摘要：凭据已换代或请求期间收到更新的摘要时不提交
    pub fn save_fetched(
        &self,
        record: &Value,
        target: &Value,
        started_at_ms: i64,
    ) -> Result<bool, String> {
        // 摘要无效或来源与凭据不一致
        if !summary::validate(record) || record["sourceId"] != target["sourceId"] {
            return Err("小组件摘要无效".into());
        }
        // 在文件锁内检查并写入
        self.locked(|| {
            if self.read_target().as_ref() != Some(target) {
                return Ok(false);
            }
            let current = self.read_summary();
            // 同一来源要求新记录不旧于现有，且现有的接收时间不晚于请求开始时间
            let accepted = current
                .as_ref()
                .and_then(summary::stamp)
                // 没有当前摘要，或者
                .is_none_or(|(source, received)| {
                    Some(source) == record["sourceId"].as_str()
                            // 新摘要不早于当前摘要
                            && record["receivedAtMs"].as_i64() >= Some(received)
                            // 且不晚于请求开始时间
                            && received <= started_at_ms
                });
            if accepted {
                self.write(SUMMARY, record)?;
            }
            Ok(accepted)
        })
    }

    /// 明确选择另一来源时撤销旧来源的摘要与凭据。
    // 明确选择另一来源时撤销旧来源的摘要与凭据
    pub fn select(&self, source_id: &str) -> Result<(), String> {
        if !summary::valid_id(source_id) {
            return Err("来源无效".into());
        }
        // 删除不属于该来源的数据
        self.drop_where(|source| source != source_id)
    }

    /// 只撤销被忘记的来源，保留另一桌面的最近同步。
    // 只撤销被忘记的来源，保留另一桌面的最近同步
    pub fn forget(&self, source_id: &str) -> Result<(), String> {
        if !summary::valid_id(source_id) {
            return Err("来源无效".into());
        }
        // 删除属于该来源的数据
        self.drop_where(|source| source == source_id)
    }

    // 清空摘要、凭据与旧文件
    pub fn clear(&self) -> Result<(), String> {
        // 加锁后执行
        self.locked(|| {
            self.remove(SUMMARY)?;
            self.remove(TARGET)?;
            self.remove_legacy();
            Ok(())
        })
    }

    // 删除来源满足条件的摘要和凭据；无法读出来源的损坏文件也一并删除
    fn drop_where(&self, matches: impl Fn(&str) -> bool) -> Result<(), String> {
        // 加锁后执行
        self.locked(|| {
            // 取出记录里的来源标识
            let source = |value: Option<Value>| {
                // 来源取字符串并复制
                value.and_then(|value| value["sourceId"].as_str().map(str::to_owned))
            };
            // 凭据存在且来源符合条件才删除
            if self.exists(TARGET) && source(self.read_target()).is_none_or(|id| matches(&id)) {
                self.remove(TARGET)?;
            }
            // 摘要存在且来源符合条件才删除
            if self.exists(SUMMARY) && source(self.read_summary()).is_none_or(|id| matches(&id)) {
                self.remove(SUMMARY)?;
            }
            Ok(())
        })
    }

    fn read_summary(&self) -> Option<Value> {
        // 按摘要大小上限读取
        self.read(SUMMARY, summary::MAX_BYTES)
            // 只保留校验通过的
            .filter(summary::validate)
    }

    // 读取并校验凭据（不超过 4 KiB）
    fn read_target(&self) -> Option<Value> {
        // 读取凭据，格式须有效
        self.read(TARGET, 4_096).filter(widget_http::valid_target)
    }

    fn write_target(&self, value: &Value) -> Result<(), String> {
        self.write(TARGET, value)
    }

    // 拼出目录下的文件路径
    fn path(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }

    // 文件（含符号链接）是否存在
    fn exists(&self, name: &str) -> bool {
        // 用不跟随链接的元数据查询
        fs::symlink_metadata(self.path(name)).is_ok()
    }

    /// 只读普通文件，拒绝符号链接与超限内容。
    // 只读普通文件，拒绝符号链接与超限内容
    fn read(&self, name: &str, limit: usize) -> Option<Value> {
        let path = self.path(name);
        // 不跟随链接地读取元数据
        let metadata = fs::symlink_metadata(&path).ok()?;
        // 不是普通文件或超过上限
        if !metadata.is_file() || metadata.len() > limit as u64 {
            return None;
        }
        let mut bytes = Vec::new();
        File::open(&path)
            // 打不开则返回 None
            .ok()?
            // 最多读比上限多一字节
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            // 读取失败则返回 None
            .ok()?;
        // 长度不超过上限才解析
        (bytes.len() <= limit)
            // 解析 JSON，失败为 None
            .then(|| serde_json::from_slice(&bytes).ok())
            // 展平两层 Option
            .flatten()
    }

    /// 临时文件写完再原子替换，进程中断不会留下半份 JSON。
    // 临时文件写完再原子替换，进程中断不会留下半份 JSON
    fn write(&self, name: &str, value: &Value) -> Result<(), String> {
        // 序列化为 JSON
        let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
        let path = self.path(name);
        // 目标不是普通文件则拒绝
        if fs::symlink_metadata(&path).is_ok_and(|metadata| !metadata.is_file()) {
            return Err("小组件共享文件不可用".into());
        }
        let temporary = self.path(&format!(".{name}.tmp"));
        // 先写入临时文件
        fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        // 再原子改名到目标
        fs::rename(&temporary, &path).map_err(|error| error.to_string())
    }

    // 删除文件；不存在也算成功
    fn remove(&self, name: &str) -> Result<(), String> {
        match fs::remove_file(self.path(name)) {
            // 出错且不是“不存在”才算失败
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
    // 在跨进程文件锁内执行；锁文件固定不删，避免不同进程锁住不同 inode；短时重试限制挂起进程的影响
    fn locked<T>(&self, body: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let unavailable = || "小组件共享目录不可用".to_owned();
        // 共享目录必须存在
        if !fs::symlink_metadata(&self.directory).is_ok_and(|metadata| metadata.is_dir()) {
            return Err(unavailable());
        }
        // 打开（必要时创建）锁文件
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            // 打开锁文件
            .open(lock_path(&self.directory))
            .map_err(|_| unavailable())?;
        let deadline = Instant::now() + Duration::from_secs(2);
        // 退避从 5 毫秒翻倍到 100 毫秒
        let mut backoff = Duration::from_millis(5);
        // 反复尝试加锁
        while file.try_lock().is_err() {
            if Instant::now() >= deadline {
                return Err(unavailable());
            }
            thread::sleep(backoff);
            // 退避时间翻倍，上限 100 毫秒
            backoff = (backoff * 2).min(Duration::from_millis(100));
        }
        let result = body();
        // 释放文件锁
        let _ = file.unlock();
        result
    }
}

// 锁文件的路径
fn lock_path(directory: &Path) -> PathBuf {
    // 目录下固定名称的锁文件
    directory.join(LOCK)
}

#[cfg(test)]
#[path = "store_test.rs"]
mod store_test;
