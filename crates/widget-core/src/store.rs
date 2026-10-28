//! App 与系统扩展共享的最近摘要和独立读取凭据。跨进程文件锁让撤销、续租与在途
//! HTTP 结果按顺序提交：迟到的结果不能覆盖更新的认证摘要，也不能恢复已撤销的来源。
// 引入摘要与 HTTP 协议
use crate::{summary, widget_http};
// 引入 JSON
use serde_json::Value;
// 引入文件系统、读写、路径、线程与时间
use std::{
    // 文件操作与打开选项
    fs::{self, File, OpenOptions},
    // 错误种类与读特征
    io::{ErrorKind, Read},
    // 路径类型
    path::{Path, PathBuf},
    // 线程
    thread,
    // 时长与时刻
    time::{Duration, Instant},
};

// 摘要文件名
const SUMMARY: &str = "widget-summary-v1.json";
// 读取凭据文件名
const TARGET: &str = "widget-target-v1.json";
// 跨进程锁文件名
const LOCK: &str = "widget.lock";
/// 原生实现留下的旧格式；新格式写入后不再读取。
// 原生实现留下的旧格式文件；新格式写入后不再读取
const LEGACY: [&str; 4] = [
    // 小组件摘要文件
    "display-widget-v1.json",
    // 手机摘要文件
    "display-phone-v1.json",
    // 刷新记录文件
    "display-widget-refresh-v1.json",
    // 锁文件
    "display-widget.lock",
];

// 小组件共享存储：主应用与系统扩展共用的最近摘要和独立读取凭据
pub struct Store {
    // 共享目录
    directory: PathBuf,
}

// 存储的方法
impl Store {
    // 以共享目录创建存储
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        // 构造
        Self {
            // 保存目录
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
        // 读取摘要并返回
        Ok(self
            // 读取摘要
            .load()?
            // 只保留仍有效的缓存
            .filter(|record| summary::is_cached(record, now_ms)))
    }

    // 读取读取凭据
    pub fn load_target(&self) -> Result<Option<Value>, String> {
        // 加锁后读取凭据
        self.locked(|| Ok(self.read_target()))
    }

    /// 认证连接的新摘要与读取凭据在同一锁内提交；同一连接重复授予保留原代次。
    // 认证连接的新摘要与读取凭据在同一把锁内提交；同一连接重复授予保留原代次
    pub fn save_authenticated(
        // 自身
        &self,
        // 摘要
        record: &Value,
        // 凭据
        target: Option<&Value>,
        // 返回是否接受
    ) -> Result<bool, String> {
        // 摘要或凭据无效，或两者来源不一致
        if !summary::validate(record)
            // 或者凭据无效
            || target.is_some_and(|value| {
                // 凭据无效或与摘要来源不一致
                !widget_http::valid_target(value) || value["sourceId"] != record["sourceId"]
            })
        {
            // 小组件摘要无效
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
                    // 同一连接则沿用
                    if widget_http::same_connection(previous, target) =>
                {
                    // 沿用旧凭据
                    Some(previous.clone())
                }
                // 其余情形取新凭据
                (target, _) => target.cloned(),
            };
            // 凭据变化才落盘
            if next != previous {
                // 按计算出的凭据处理
                match &next {
                    // 有则写入
                    Some(value) => self.write_target(value)?,
                    // 没有新凭据则删除
                    None => self.remove(TARGET)?,
                }
            }
            // 同一来源的摘要不得被更旧的接收时间覆盖
            let accepted = self
                // 读取当前摘要
                .read_summary()
                // 取引用
                .as_ref()
                // 取来源与接收时间
                .and_then(summary::stamp)
                // 没有当前摘要，或者
                .is_none_or(|(source, received)| {
                    // 来源与摘要不一致
                    Some(source) != record["sourceId"].as_str()
                        // 新摘要不早于当前摘要
                        || record["receivedAtMs"].as_i64() >= Some(received)
                });
            // 摘要被接受才写入
            if accepted {
                // 写入摘要
                self.write(SUMMARY, record)?;
                // 并清理旧格式文件
                self.remove_legacy();
            }
            // 返回是否被接受
            Ok(accepted)
        })
    }

    /// 心跳只续当前来源仍在租约内的摘要，过期摘要不能被重新标为在线。
    // 心跳只续当前来源仍在租约内的摘要，过期的摘要不能被重新标为在线
    pub fn renew(&self, source_id: &str, now_ms: i64) -> Result<bool, String> {
        // 加锁后执行
        self.locked(|| {
            // 读取当前摘要
            let Some(current) = self
                // 读取当前摘要
                .read_summary()
                // 来源须一致
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
        // 自身
        &self,
        // 摘要
        record: &Value,
        // 凭据
        target: &Value,
        // 请求开始时间
        started_at_ms: i64,
        // 返回是否接受
    ) -> Result<bool, String> {
        // 摘要无效或来源与凭据不一致
        if !summary::validate(record) || record["sourceId"] != target["sourceId"] {
            // 小组件摘要无效
            return Err("小组件摘要无效".into());
        }
        // 在文件锁内检查并写入
        self.locked(|| {
            // 凭据已变化
            if self.read_target().as_ref() != Some(target) {
                // 不接受
                return Ok(false);
            }
            // 读取当前摘要
            let current = self.read_summary();
            // 同一来源要求新记录不旧于现有，且现有的接收时间不晚于请求开始时间
            let accepted =
                // 当前摘要
                current
                    // 取引用
                    .as_ref()
                    // 取来源与接收时间
                    .and_then(summary::stamp)
                    // 没有当前摘要，或者
                    .is_none_or(|(source, received)| {
                        // 来源与新摘要一致
                        Some(source) == record["sourceId"].as_str()
                            // 新摘要不早于当前摘要
                            && record["receivedAtMs"].as_i64() >= Some(received)
                            // 且不晚于请求开始时间
                            && received <= started_at_ms
                    });
            // 被接受才写入
            if accepted {
                // 写入摘要
                self.write(SUMMARY, record)?;
            }
            // 返回是否被接受
            Ok(accepted)
        })
    }

    /// 明确选择另一来源时撤销旧来源的摘要与凭据。
    // 明确选择另一来源时撤销旧来源的摘要与凭据
    pub fn select(&self, source_id: &str) -> Result<(), String> {
        // 来源标识无效
        if !summary::valid_id(source_id) {
            // 来源无效
            return Err("来源无效".into());
        }
        // 删除不属于该来源的数据
        self.drop_where(|source| source != source_id)
    }

    /// 只撤销被忘记的来源，保留另一桌面的最近同步。
    // 只撤销被忘记的来源，保留另一桌面的最近同步
    pub fn forget(&self, source_id: &str) -> Result<(), String> {
        // 来源标识无效
        if !summary::valid_id(source_id) {
            // 来源无效
            return Err("来源无效".into());
        }
        // 删除属于该来源的数据
        self.drop_where(|source| source == source_id)
    }

    // 清空摘要、凭据与旧文件
    pub fn clear(&self) -> Result<(), String> {
        // 加锁后执行
        self.locked(|| {
            // 删除摘要
            self.remove(SUMMARY)?;
            // 删除凭据
            self.remove(TARGET)?;
            // 清理旧格式文件
            self.remove_legacy();
            // 完成
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
                // 删除凭据
                self.remove(TARGET)?;
            }
            // 摘要存在且来源符合条件才删除
            if self.exists(SUMMARY) && source(self.read_summary()).is_none_or(|id| matches(&id)) {
                // 删除摘要
                self.remove(SUMMARY)?;
            }
            // 完成
            Ok(())
        })
    }

    // 读取并校验摘要
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

    // 写入读取凭据
    fn write_target(&self, value: &Value) -> Result<(), String> {
        // 落盘到凭据文件
        self.write(TARGET, value)
    }

    // 拼出目录下的文件路径
    fn path(&self, name: &str) -> PathBuf {
        // 目录加文件名
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
        // 文件路径
        let path = self.path(name);
        // 不跟随链接地读取元数据
        let metadata = fs::symlink_metadata(&path).ok()?;
        // 不是普通文件或超过上限
        if !metadata.is_file() || metadata.len() > limit as u64 {
            // 返回 None
            return None;
        }
        // 读取缓冲区
        let mut bytes = Vec::new();
        // 打开文件
        File::open(&path)
            // 打不开则返回 None
            .ok()?
            // 最多读比上限多一字节
            .take(limit as u64 + 1)
            // 读入缓冲区
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
        // 目标路径
        let path = self.path(name);
        // 目标不是普通文件则拒绝
        if fs::symlink_metadata(&path).is_ok_and(|metadata| !metadata.is_file()) {
            // 共享文件不可用
            return Err("小组件共享文件不可用".into());
        }
        // 临时文件路径
        let temporary = self.path(&format!(".{name}.tmp"));
        // 先写入临时文件
        fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        // 再原子改名到目标
        fs::rename(&temporary, &path).map_err(|error| error.to_string())
    }

    // 删除文件；不存在也算成功
    fn remove(&self, name: &str) -> Result<(), String> {
        // 删除文件
        match fs::remove_file(self.path(name)) {
            // 出错且不是“不存在”才算失败
            Err(error) if error.kind() != ErrorKind::NotFound => Err(error.to_string()),
            // 其余情况视为成功
            _ => Ok(()),
        }
    }

    // 删除旧格式文件
    fn remove_legacy(&self) {
        // 遍历旧格式文件名
        for name in LEGACY {
            // 删除，失败忽略
            let _ = self.remove(name);
        }
    }

    /// 固定锁文件不删除，避免不同进程锁住不同 inode；短时重试限制挂起进程的影响。
    // 在跨进程文件锁内执行；锁文件固定不删，避免不同进程锁住不同 inode；短时重试限制挂起进程的影响
    fn locked<T>(&self, body: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        // 目录不可用的错误
        let unavailable = || "小组件共享目录不可用".to_owned();
        // 共享目录必须存在
        if !fs::symlink_metadata(&self.directory).is_ok_and(|metadata| metadata.is_dir()) {
            // 报目录不可用
            return Err(unavailable());
        }
        // 打开（必要时创建）锁文件
        let file = OpenOptions::new()
            // 不存在则创建
            .create(true)
            // 不截断
            .truncate(false)
            // 可读
            .read(true)
            // 可写
            .write(true)
            // 打开锁文件
            .open(lock_path(&self.directory))
            // 失败报目录不可用
            .map_err(|_| unavailable())?;
        // 最多重试 2 秒
        let deadline = Instant::now() + Duration::from_secs(2);
        // 退避从 5 毫秒翻倍到 100 毫秒
        let mut backoff = Duration::from_millis(5);
        // 反复尝试加锁
        while file.try_lock().is_err() {
            // 超过期限
            if Instant::now() >= deadline {
                // 报目录不可用
                return Err(unavailable());
            }
            // 休眠后重试
            thread::sleep(backoff);
            // 退避时间翻倍，上限 100 毫秒
            backoff = (backoff * 2).min(Duration::from_millis(100));
        }
        // 执行调用方的操作
        let result = body();
        // 释放文件锁
        let _ = file.unlock();
        // 返回操作结果
        result
    }
}

// 锁文件的路径
fn lock_path(directory: &Path) -> PathBuf {
    // 目录下固定名称的锁文件
    directory.join(LOCK)
}

// 测试模块
#[cfg(test)]
#[path = "store_test.rs"]
mod store_test;
