//! 由进程持有的 WatchConnectivity 投递,传送公开的 Rust 缓存,独立于 NSH VM。
mod files;
mod policy;
// WatchConnectivity 绑定
mod session;

use super::{Manifest, read_manifest};
use crate::native::lifecycle;
use objc2::rc::{Retained, autoreleasepool};
use objc2_foundation::{NSDictionary, NSString, NSURL};
use policy::Retry;
use session::{Delegate, WCSession};
// 标准库集合与同步原语
use std::{
    // 有序映射与有序集合
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        // 互斥锁、一次性初始化与全局单例
        Mutex,
        // 一次性执行
        Once,
        // 一次性初始化
        OnceLock,
        // 通道的接收端与发送端
        mpsc::{self, Receiver, SyncSender},
    },
    // 线程
    thread,
    time::{Duration, Instant},
};

mod flush;
mod presence;
pub(in crate::native) use presence::command as presence_command;

/// 操作系统持有排队的传输;应用从不构建无限增长的文件积压。
// 操作系统拥有排队的传输；应用不会堆出无界的文件积压
const MAX_TRANSFERS: usize = 2;
// 完成回调的待处理上限
const MAX_COMPLETIONS: usize = 64;

#[derive(Clone)]
// 一次文件传输：流、修订号、摘要与暂存路径
struct Transfer {
    stream: String,
    revision: u64,
    hash: String,
    path: PathBuf,
}

#[derive(Default)]
// 回调线程交给工作线程的信号
struct Signals {
    root: Option<PathBuf>,
    reset: bool,
    activated: Option<bool>,
    // 已完成的传输及其成败
    completed: Vec<(Transfer, bool)>,
    // 最新LAN探测/撤销意图及有界WC终态、前台撤销数据。
    presence: Option<crate::native::companion::presence::Command>,
    presence_completed: Vec<(String, Option<Vec<u8>>)>,
    presence_revoked: Vec<Vec<u8>>,
    // 邮箱满时不能丢掉撤销事实后仍接纳迟到的在线回执。
    presence_revoked_overflow: bool,
}

static SIGNALS: Mutex<Signals> = Mutex::new(Signals {
    // 默认没有新的根目录
    root: None,
    reset: false,
    activated: None,
    completed: Vec::new(),
    presence: None,
    presence_completed: Vec::new(),
    presence_revoked: Vec::new(),
    presence_revoked_overflow: false,
});
// 唤醒工作线程的通道
static WAKE: OnceLock<SyncSender<()>> = OnceLock::new();

/// 注册进程启动回调;JavaScript 和场景都不会启动该会话。
// 注册进程启动回调；JavaScript 和场景都不会启动会话
pub(crate) fn install() {
    // 安装只执行一次
    static INSTALL: Once = Once::new();
    // 安装生命周期观察，并启动执行者
    INSTALL.call_once(|| lifecycle::install(start));
}

/// 合并所有者的提交。只有根路径会越过这条边界,绝不传递图片字节或机密。
// 合并 owner 的提交；穿过这个边界的只有根路径，没有图片字节或秘密
pub(super) fn changed(root: &Path) {
    SIGNALS
        // 加锁
        .lock()
        // 锁中毒时仍取出内部值
        .unwrap_or_else(|error| error.into_inner())
        .root = Some(root.to_owned());
    wake();
}

// 唤醒工作线程（队列满即已有唤醒）
fn wake() {
    if let Some(tx) = WAKE.get() {
        // 非阻塞地发送唤醒，满了就忽略
        let _ = tx.try_send(());
    }
}

// 系统状态变化：要求整体重新对账
pub(super) fn system_changed() {
    SIGNALS
        // 加锁
        .lock()
        // 锁中毒时仍取出内部值
        .unwrap_or_else(|error| error.into_inner())
        .reset = true;
    wake();
}

fn activation_finished(success: bool) {
    SIGNALS
        // 加锁
        .lock()
        // 锁中毒时仍取出内部值
        .unwrap_or_else(|error| error.into_inner())
        .activated = Some(success);
    wake();
}

// 记录传输完成；回调过多时只做一次整体对账，不再排队更多工作
fn finished(transfer: Transfer, success: bool) {
    // 取锁，中毒时仍取出内部值
    let mut signals = SIGNALS.lock().unwrap_or_else(|error| error.into_inner());
    // 未超过完成记录上限
    if signals.completed.len() < MAX_COMPLETIONS {
        // 记录这次传输及其结果
        signals.completed.push((transfer, success));
    // 超过上限时丢弃最旧的记录
    } else {
        // 大量过时回调只需要一次调和,而不是更多排队的工作。
        signals.reset = true;
    }
    // 先释放锁
    drop(signals);
    // 再唤醒执行者
    wake();
}

// 固定的公开缓存目录
fn default_root() -> Option<PathBuf> {
    // 由 HOME 环境变量得到用户目录
    let home = PathBuf::from(std::env::var_os("HOME")?);
    // 必须是绝对路径
    home.is_absolute()
        .then(|| home.join("Library/Application Support/loki-link-v1/companion-v1"))
}

// 启动工作线程（只执行一次）
fn start() {
    // 启动只执行一次
    static START: Once = Once::new();
    // 在回调中完成初始化
    START.call_once(|| {
        // 没有默认根目录则放弃
        let Some(root) = default_root() else { return };
        // 容量为 1 的唤醒通道
        let (tx, rx) = mpsc::sync_channel(1);
        let _ = WAKE.set(tx);
        // 用线程构造器创建后台线程
        let _ = thread::Builder::new()
            // 线程名
            .name("loki-watch-cache".into())
            // 在线程里执行
            .spawn(move || {
                autoreleasepool(|_| {
                    if WCSession::is_supported() {
                        // 创建执行者并进入循环
                        Actor::new(root).run(rx);
                    }
                });
            });
    });
}

// 工作线程的全部状态
struct Actor {
    // 即时前台回执与背景文件传输各自拥有状态；序号跨LAN代次保留。
    presence: crate::native::companion::presence::Controller,
    root: PathBuf,
    session: Retained<WCSession>,
    _delegate: Retained<Delegate>,
    manifest: Option<Manifest>,
    // 已发送的上下文字节
    context: Option<Vec<u8>>,
    context_retry: Retry,
    activation_retry: Retry,
    activation_deadline: Option<Instant>,
    successful: BTreeSet<String>,
    attempts: BTreeMap<String, Retry>,
    // 本地读取失败的资源
    local_failures: BTreeSet<String>,
    in_flight: BTreeMap<PathBuf, Transfer>,
}

impl Actor {
    // 创建会话与代理并挂接
    fn new(root: PathBuf) -> Self {
        let session = WCSession::shared();
        let delegate = Delegate::new();
        session.attach(&delegate);
        Self {
            presence: Default::default(),
            root,
            session,
            _delegate: delegate,
            manifest: None,
            context: None,
            context_retry: Retry::default(),
            activation_retry: Retry::default(),
            activation_deadline: None,
            successful: BTreeSet::new(),
            attempts: BTreeMap::new(),
            local_failures: BTreeSet::new(),
            in_flight: BTreeMap::new(),
        }
    }

    // 主循环：处理信号、推进同步，再等待下一个唤醒或重试期限
    fn run(mut self, rx: Receiver<()>) {
        loop {
            let signals =
                std::mem::take(&mut *SIGNALS.lock().unwrap_or_else(|error| error.into_inner()));
            // 应用本轮收到的信号
            self.apply(signals);
            // 在自动释放池内执行一轮工作
            autoreleasepool(|_| {
                self.flush();
                self.flush_presence();
            });
            // 计算最近的重试期限
            let deadline = self
                .attempts
                .values()
                // 取各自的下次重试时刻
                .filter_map(Retry::deadline)
                // 加上上下文重试时刻
                .chain(self.context_retry.deadline())
                .chain(self.activation_retry.deadline())
                .chain(self.activation_deadline)
                .chain(self.presence.deadline())
                // 只保留尚未到的时刻
                .filter(|deadline| *deadline > Instant::now())
                .min();
            // 没有待处理的重试时,工作线程无限期睡眠。挂起由 iOS 负责。
            // 没有待重试的工作就无限期休眠，挂起由 iOS 管理
            match deadline {
                // 有期限：等到期限或被唤醒
                Some(deadline) => {
                    // 按距期限的剩余时间等待
                    let _ = rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
                }
                // 没有期限：一直等待唤醒
                None => {
                    if rx.recv().is_err() {
                        return;
                    }
                }
            }
        }
    }

    fn apply(&mut self, signals: Signals) {
        if let Some(command) = signals.presence {
            self.presence.command(command);
        }
        // 同轮撤销优先于迟到positive回执，避免旧前台短暂复活。
        self.presence.revoke_batch(
            signals.presence_revoked,
            signals.presence_revoked_overflow,
            Instant::now(),
        );
        for (challenge, bytes) in signals.presence_completed {
            self.presence
                .complete(&challenge, bytes.as_deref(), Instant::now());
        }
        if let Some(root) = signals.root {
            // 产品只有一个固定的公开缓存;拒绝意外的任意路径读取。
            // 产品只有一个固定的公开缓存；拒绝任意路径读取
            if default_root().as_ref() == Some(&root) {
                self.root = root;
                // 新提交的图片字节可能在清单本身之后才到达。
                // 本地读取失败不得让该图片永久耗尽重试机会。
                // 新提交的图片字节可能晚于清单到达；本地读取失败不应让该图片永久耗尽预算
                self.local_failures.retain(|hash| {
                    // 检查资源文件是否已到达
                    let arrived = fs::symlink_metadata(self.root.join("assets").join(hash))
                        // 是普通文件且不是符号链接
                        .is_ok_and(|meta| meta.is_file() && !meta.file_type().is_symlink());
                    if arrived {
                        self.attempts.remove(hash);
                    }
                    !arrived
                });
            }
        }
        if signals.reset {
            self.context = None;
            self.context_retry = Retry::default();
            self.activation_retry = Retry::default();
            self.successful.clear();
            self.attempts.clear();
            self.local_failures.clear();
            // 恢复之后以操作系统队列为准,包括在本进程
            // 被挂起或不可用期间已经完成的回调。
            // 恢复后以系统队列为准，包括进程挂起期间已完成的回调
            self.in_flight.clear();
        }
        if let Some(success) = signals.activated {
            self.activation_deadline = None;
            // 成功：重置重试并重发上下文
            if success {
                self.activation_retry = Retry::default();
                self.context = None;
                self.context_retry = Retry::default();
            } else {
                // 激活失败，登记一次失败
                self.activation_retry.failed(Instant::now());
            }
        }
        for (transfer, success) in signals.completed {
            // 是否是本执行者发起的传输
            let known = self.in_flight.remove(&transfer.path).is_some();
            files::remove(&self.root, &transfer.path);
            // 未知或与当前清单不兼容的结果忽略
            if !known
                || !self
                    .manifest
                    .as_ref()
                    .is_some_and(|manifest| compatible(&transfer, manifest))
            {
                continue;
            }
            // 成功：记录并清除重试
            if success {
                self.successful.insert(transfer.hash.clone());
                self.attempts.remove(&transfer.hash);
            } else {
                // 累计该资源的失败次数
                self.attempts
                    .entry(transfer.hash)
                    .or_default()
                    // 登记一次失败
                    .failed(Instant::now());
            }
        }
    }
}

// 传输是否与清单兼容：同流、修订号不超过当前且被引用
fn compatible(transfer: &Transfer, manifest: &Manifest) -> bool {
    // 按策略判断是否可复用
    policy::reusable(
        &transfer.stream,
        transfer.revision,
        &manifest.stream_id,
        manifest.revision.parse().unwrap_or(0),
        manifest
            .assets
            .iter()
            // 哈希与传输一致才仍有效
            .any(|asset| asset.hash == transfer.hash),
    )
}
