//! 由进程持有的 WatchConnectivity 投递,传送公开的 Rust 缓存,独立于 NSH VM。
// 暂存文件
mod files;
// 重试策略
mod policy;
// WatchConnectivity 绑定
mod session;

// 引入清单与读取函数
use super::{Manifest, read_manifest};
// 引入生命周期
use crate::native::lifecycle;
// 引入自动释放池
use objc2::rc::{Retained, autoreleasepool};
// 引入 Foundation 类型
use objc2_foundation::{NSDictionary, NSString, NSURL};
// 引入重试策略
use policy::Retry;
// 引入会话委托与会话类型
use session::{Delegate, WCSession};
// 标准库集合与同步原语
use std::{
    // 有序映射与有序集合
    collections::{BTreeMap, BTreeSet},
    // 文件系统
    fs,
    // 路径类型
    path::{Path, PathBuf},
    // 同步原语
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
    // 时长与时刻
    time::{Duration, Instant},
};

mod flush;

/// 操作系统持有排队的传输;应用从不构建无限增长的文件积压。
// 操作系统拥有排队的传输；应用不会堆出无界的文件积压
const MAX_TRANSFERS: usize = 2;
// 完成回调的待处理上限
const MAX_COMPLETIONS: usize = 64;

#[derive(Clone)]
// 一次文件传输：流、修订号、摘要与暂存路径
struct Transfer {
    // 所属流标识
    stream: String,
    // 修订号
    revision: u64,
    // 资源哈希
    hash: String,
    // 文件路径
    path: PathBuf,
}

#[derive(Default)]
// 回调线程交给工作线程的信号
struct Signals {
    // 发件箱根目录
    root: Option<PathBuf>,
    // 需要整体重新对账
    reset: bool,
    // 激活结果
    activated: Option<bool>,
    // 已完成的传输及其成败
    completed: Vec<(Transfer, bool)>,
}

// 全局信号
static SIGNALS: Mutex<Signals> = Mutex::new(Signals {
    // 默认没有新的根目录
    root: None,
    // 尚未重置
    reset: false,
    // 激活结果未知
    activated: None,
    // 完成记录为空
    completed: Vec::new(),
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
    // 取全局信号
    SIGNALS
        // 加锁
        .lock()
        // 锁中毒时仍取出内部值
        .unwrap_or_else(|error| error.into_inner())
        // 记录新的根目录
        .root = Some(root.to_owned());
    // 唤醒执行者
    wake();
}

// 唤醒工作线程（队列满即已有唤醒）
fn wake() {
    // 有唤醒通道
    if let Some(tx) = WAKE.get() {
        // 非阻塞地发送唤醒，满了就忽略
        let _ = tx.try_send(());
    }
}

// 系统状态变化：要求整体重新对账
fn system_changed() {
    // 取全局信号
    SIGNALS
        // 加锁
        .lock()
        // 锁中毒时仍取出内部值
        .unwrap_or_else(|error| error.into_inner())
        // 标记需要重置
        .reset = true;
    // 唤醒执行者
    wake();
}

// 记录激活结果
fn activation_finished(success: bool) {
    // 取全局信号
    SIGNALS
        // 加锁
        .lock()
        // 锁中毒时仍取出内部值
        .unwrap_or_else(|error| error.into_inner())
        // 记录激活结果
        .activated = Some(success);
    // 唤醒执行者
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
        // 拼出配套目录
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
        // 保存发送端
        let _ = WAKE.set(tx);
        // 新的前台窗口可以重试此前耗尽预算的工作。
        // 回到前台时可重试此前耗尽预算的工作
        static ACTIVITY: OnceLock<lifecycle::ActivityObserver> = OnceLock::new();
        // 登记前后台观察者
        let _ = ACTIVITY.set(lifecycle::observe(|visible| {
            // 应用可见时
            if visible {
                // 通知系统状态有变化
                system_changed();
            }
        }));
        // 用线程构造器创建后台线程
        let _ = thread::Builder::new()
            // 线程名
            .name("loki-watch-cache".into())
            // 在线程里执行
            .spawn(move || {
                // 放进自动释放池
                autoreleasepool(|_| {
                    // 设备支持才运行
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
    // 根目录
    root: PathBuf,
    // 会话
    session: Retained<WCSession>,
    // 代理，保持存活
    _delegate: Retained<Delegate>,
    // 当前清单
    manifest: Option<Manifest>,
    // 已发送的上下文字节
    context: Option<Vec<u8>>,
    // 上下文发送的重试
    context_retry: Retry,
    // 激活的重试
    activation_retry: Retry,
    // 激活期限
    activation_deadline: Option<Instant>,
    // 已成功传输的资源
    successful: BTreeSet<String>,
    // 各资源的重试状态
    attempts: BTreeMap<String, Retry>,
    // 本地读取失败的资源
    local_failures: BTreeSet<String>,
    // 正在传输的文件
    in_flight: BTreeMap<PathBuf, Transfer>,
}

// 执行者的方法
impl Actor {
    // 创建会话与代理并挂接
    fn new(root: PathBuf) -> Self {
        // 取共享会话
        let session = WCSession::shared();
        // 创建委托
        let delegate = Delegate::new();
        // 把委托挂到会话上
        session.attach(&delegate);
        // 构造执行者
        Self {
            // 根目录
            root,
            // 会话
            session,
            // 保持委托存活
            _delegate: delegate,
            // 尚无清单
            manifest: None,
            // 尚无上下文
            context: None,
            // 上下文重试策略
            context_retry: Retry::default(),
            // 激活重试策略
            activation_retry: Retry::default(),
            // 尚无激活期限
            activation_deadline: None,
            // 已成功的传输
            successful: BTreeSet::new(),
            // 各传输的尝试次数
            attempts: BTreeMap::new(),
            // 本地失败的传输
            local_failures: BTreeSet::new(),
            // 进行中的传输
            in_flight: BTreeMap::new(),
        }
    }

    // 主循环：处理信号、推进同步，再等待下一个唤醒或重试期限
    fn run(mut self, rx: Receiver<()>) {
        // 主循环
        loop {
            // 取走累积的信号
            let signals =
                // 取走全部信号
                std::mem::take(&mut *SIGNALS.lock().unwrap_or_else(|error| error.into_inner()));
            // 应用本轮收到的信号
            self.apply(signals);
            // 在自动释放池内执行一轮工作
            autoreleasepool(|_| self.flush());
            // 计算最近的重试期限
            let deadline = self
                // 各资源的重试策略
                .attempts
                // 取其值
                .values()
                // 取各自的下次重试时刻
                .filter_map(Retry::deadline)
                // 加上上下文重试时刻
                .chain(self.context_retry.deadline())
                // 加上激活重试时刻
                .chain(self.activation_retry.deadline())
                // 加上激活期限
                .chain(self.activation_deadline)
                // 只保留尚未到的时刻
                .filter(|deadline| *deadline > Instant::now())
                // 取最早的时刻
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
                    // 通道关闭
                    if rx.recv().is_err() {
                        // 结束循环
                        return;
                    }
                }
            }
        }
    }

    // 应用信号到状态
    fn apply(&mut self, signals: Signals) {
        // 有新的根目录
        if let Some(root) = signals.root {
            // 产品只有一个固定的公开缓存;拒绝意外的任意路径读取。
            // 产品只有一个固定的公开缓存；拒绝任意路径读取
            if default_root().as_ref() == Some(&root) {
                // 替换根目录
                self.root = root;
                // 新提交的图片字节可能在清单本身之后才到达。
                // 本地读取失败不得让该图片永久耗尽重试机会。
                // 新提交的图片字节可能晚于清单到达；本地读取失败不应让该图片永久耗尽预算
                self.local_failures.retain(|hash| {
                    // 检查资源文件是否已到达
                    let arrived = fs::symlink_metadata(self.root.join("assets").join(hash))
                        // 是普通文件且不是符号链接
                        .is_ok_and(|meta| meta.is_file() && !meta.file_type().is_symlink());
                    // 已到达
                    if arrived {
                        // 清除其尝试记录
                        self.attempts.remove(hash);
                    }
                    // 保留尚未到达的
                    !arrived
                });
            }
        }
        // 整体重置各项状态
        if signals.reset {
            // 清空上下文
            self.context = None;
            // 重置上下文重试
            self.context_retry = Retry::default();
            // 重置激活重试
            self.activation_retry = Retry::default();
            // 清空已成功的传输
            self.successful.clear();
            // 清空尝试次数
            self.attempts.clear();
            // 清空本地失败
            self.local_failures.clear();
            // 恢复之后以操作系统队列为准,包括在本进程
            // 被挂起或不可用期间已经完成的回调。
            // 恢复后以系统队列为准，包括进程挂起期间已完成的回调
            self.in_flight.clear();
        }
        // 处理激活结果
        if let Some(success) = signals.activated {
            // 清除激活期限
            self.activation_deadline = None;
            // 成功：重置重试并重发上下文
            if success {
                // 重置激活重试
                self.activation_retry = Retry::default();
                // 清空上下文
                self.context = None;
                // 重置上下文重试
                self.context_retry = Retry::default();
            // 失败：计入退避
            } else {
                // 激活失败，登记一次失败
                self.activation_retry.failed(Instant::now());
            }
        }
        // 处理已完成的传输
        for (transfer, success) in signals.completed {
            // 是否是本执行者发起的传输
            let known = self.in_flight.remove(&transfer.path).is_some();
            // 删除暂存文件
            files::remove(&self.root, &transfer.path);
            // 未知或与当前清单不兼容的结果忽略
            if !known
                // 或者
                || !self
                    // 当前清单
                    .manifest
                    // 取引用
                    .as_ref()
                    // 与传输不兼容
                    .is_some_and(|manifest| compatible(&transfer, manifest))
            {
                // 跳过
                continue;
            }
            // 成功：记录并清除重试
            if success {
                // 记录传输成功
                self.successful.insert(transfer.hash.clone());
                // 清除尝试记录
                self.attempts.remove(&transfer.hash);
            // 失败：计入退避
            } else {
                // 累计该资源的失败次数
                self.attempts
                    // 按资源哈希取条目
                    .entry(transfer.hash)
                    // 默认策略
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
        // 传输的流
        &transfer.stream,
        // 传输的修订号
        transfer.revision,
        // 清单的流
        &manifest.stream_id,
        // 清单的修订号
        manifest.revision.parse().unwrap_or(0),
        // 清单
        manifest
            // 资源
            .assets
            // 遍历清单资源
            .iter()
            // 哈希与传输一致才仍有效
            .any(|asset| asset.hash == transfer.hash),
    )
}
