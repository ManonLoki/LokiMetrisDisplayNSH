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

    // 推进同步：激活会话、发送上下文、传输缺失的图片
    fn flush(&mut self) {
        // 当前时刻
        let now = Instant::now();
        // 会话需要激活时
        if self
            // 激活期限
            .activation_deadline
            // 已到
            .is_some_and(|deadline| now >= deadline)
        {
            // 清除激活期限
            self.activation_deadline = None;
            // 登记激活失败
            self.activation_retry.failed(now);
        }
        // 未激活：仅在未激活（0）状态才发起激活；不活跃（1）需先完成停用，才能为另一块手表重新激活
        if self.session.activation_state() != 2 {
            // Inactive (1) 必须先完成停用,才能为另一只手表重新激活。
            if self.session.activation_state() == 0
                // 且没有激活期限
                && self.activation_deadline.is_none()
                // 且激活重试已就绪
                && self.activation_retry.ready(now)
            {
                // 记录激活已开始
                self.activation_retry.started();
                // 设 15 秒的激活期限
                self.activation_deadline = Some(now + Duration::from_secs(15));
                // 激活会话
                self.session.activate();
            }
            // 然后返回
            return;
        }
        // 没有配对手表或手表应用未安装：取消所有传输并清除上下文
        if !self.session.paired() || !self.session.installed() {
            // 取消所有进行中的传输
            self.cancel_outstanding(None);
            // 清空上下文
            self.context = None;
            // 结束本轮
            return;
        }
        // 清单不可读则等待下次
        let Ok(manifest) = read_manifest(&self.root) else {
            // 结束本轮
            return;
        };
        // 数据流或来源变化
        let switched = self.manifest.as_ref().is_some_and(|previous| {
            // 流或来源与之前不同
            previous.stream_id != manifest.stream_id || previous.source_id != manifest.source_id
        });
        // 切换来源或清空：取消全部传输并清空状态
        if switched || manifest.snapshot.is_none() {
            // 取消所有进行中的传输
            self.cancel_outstanding(None);
            // 清空已成功的传输
            self.successful.clear();
            // 清空尝试次数
            self.attempts.clear();
            // 清空本地失败
            self.local_failures.clear();
        // 否则只取消与新清单不兼容的传输
        } else {
            // 只取消不在新清单里的传输
            self.cancel_outstanding(Some(&manifest));
        }
        // 清单变化：重发上下文，并裁剪不再引用的资源状态
        if self.manifest.as_ref() != Some(&manifest) {
            // 清空上下文
            self.context = None;
            // 重置上下文重试
            self.context_retry = Retry::default();
            // 已成功的传输
            self.successful
                // 只保留清单里仍有的哈希
                .retain(|hash| manifest.assets.iter().any(|asset| &asset.hash == hash));
            // 尝试次数
            self.attempts
                // 只保留清单里仍有的传输
                .retain(|hash, _| manifest.assets.iter().any(|asset| &asset.hash == hash));
            // 本地失败
            self.local_failures
                // 只保留仍在清单里的资源
                .retain(|hash| manifest.assets.iter().any(|asset| &asset.hash == hash));
            // 记下新清单
            self.manifest = Some(manifest.clone());
        }
        // 发送应用上下文（不超过 48 KiB）；失败则退避
        if self.context.is_none() && self.context_retry.ready(now) {
            // 记录上下文发送开始
            self.context_retry.started();
            // 序列化清单
            if let Ok(bytes) = serde_json::to_vec(&manifest) {
                // 不超过 48 KiB 且发送成功
                if bytes.len() <= 48 * 1024 && self.session.context(&bytes).is_ok() {
                    // 记住已发送的上下文
                    self.context = Some(bytes);
                // 否则
                } else {
                    // 登记失败，稍后重试
                    self.context_retry.failed(now);
                }
            }
        }
        // 已在传输的摘要
        let mut occupied: BTreeSet<_> = self
            // 进行中的传输
            .in_flight
            // 取其值
            .values()
            // 取哈希
            .map(|transfer| transfer.hash.clone())
            // 收集为集合
            .collect();
        // 逐个资源安排传输，最多同时 MAX_TRANSFERS 个
        for asset in &manifest.assets {
            // 进行中的传输已达上限
            if self.in_flight.len() >= MAX_TRANSFERS {
                // 停止发起新传输
                break;
            }
            // 已成功、传输中或尚在退避的跳过
            if self.successful.contains(&asset.hash)
                // 或者该哈希已被占用
                || occupied.contains(&asset.hash)
                // 或者
                || !self
                    // 尝试记录
                    .attempts
                    // 按哈希取条目
                    .entry(asset.hash.clone())
                    // 默认策略
                    .or_default()
                    // 尚未到重试时刻
                    .ready(now)
            {
                // 跳过本项
                continue;
            }
            // 记录开始一次尝试
            self.attempts
                // 按哈希取条目
                .entry(asset.hash.clone())
                // 默认策略
                .or_default()
                // 记录开始尝试
                .started();
            // 暂存不可变副本
            match files::stage(&self.root, asset) {
                // 文件准备成功
                Ok(path) => {
                    // 清除本地失败记录
                    self.local_failures.remove(&asset.hash);
                    // 暂存可能耗时;所有者上的来源切换必须优先。
                    // 暂存可能耗时；期间 owner 切换了来源就丢弃，以新状态为准
                    if !read_manifest(&self.root).is_ok_and(|latest| {
                        // 最新清单仍属于同一个流
                        latest.stream_id == manifest.stream_id
                            // 来源一致
                            && latest.source_id == manifest.source_id
                            // 资源仍在清单里
                            && latest.assets.iter().any(|candidate| candidate == asset)
                        // 条件结束
                    }) {
                        // 否则删除这份暂存文件
                        files::remove(&self.root, &path);
                        // 唤醒执行者
                        wake();
                        // 放弃本次传输
                        return;
                    }
                    // 构造传输描述
                    let transfer = Transfer {
                        // 所属流
                        stream: manifest.stream_id.clone(),
                        // 修订号
                        revision: manifest.revision.parse().unwrap_or(0),
                        // 资源哈希
                        hash: asset.hash.clone(),
                        // 文件路径
                        path,
                    };
                    // 传输元数据：类型、流 ID、修订号与摘要
                    let keys: Vec<_> = ["kind", "streamId", "revision", "hash"]
                        // 遍历
                        .iter()
                        // 转为系统字符串
                        .map(|key| NSString::from_str(key))
                        // 收集为列表
                        .collect();
                    // 键与值的列表
                    let values: Vec<_> = [
                        // 种类：资源
                        "asset",
                        // 流标识
                        &manifest.stream_id,
                        // 修订号
                        &manifest.revision,
                        // 资源哈希
                        &asset.hash,
                    ]
                    // 遍历
                    .iter()
                    // 转为系统字符串
                    .map(|value| NSString::from_str(value))
                    // 收集为列表
                    .collect();
                    // 构造元数据字典
                    let metadata = NSDictionary::<NSString, NSString>::from_slices(
                        // 键列表
                        &keys.iter().map(|key| &**key).collect::<Vec<_>>(),
                        // 值列表
                        &values.iter().map(|value| &**value).collect::<Vec<_>>(),
                    );
                    // 文件 URL
                    let file = NSURL::fileURLWithPath(&NSString::from_str(
                        // 来自传输路径
                        &transfer.path.to_string_lossy(),
                    ));
                    // 交给系统传输
                    let _ = self.session.transfer_file(&file, Some(&metadata));
                    // 标记该资源有传输占用
                    occupied.insert(transfer.hash.clone());
                    // 登记进行中的传输
                    self.in_flight.insert(transfer.path.clone(), transfer);
                }
                // 暂存失败：记为本地失败并退避
                Err(_) => {
                    // 标记本地失败
                    self.local_failures.insert(asset.hash.clone());
                    // 累计尝试次数
                    self.attempts
                        // 按哈希取条目
                        .entry(asset.hash.clone())
                        // 默认策略
                        .or_default()
                        // 登记失败
                        .failed(now);
                }
            }
        }
    }

    // 取消不兼容的系统传输，保留与清单兼容的，并清理多余的暂存文件
    fn cancel_outstanding(&mut self, manifest: Option<&Manifest>) {
        // 系统里未完成的传输
        let outstanding = self.session.outstanding();
        // 需要保留的文件路径
        let mut retained = BTreeSet::new();
        // 遍历未完成传输
        for index in 0..outstanding.count() {
            // 取出一个传输
            let transfer = outstanding.objectAtIndex(index);
            // 读取其描述
            let Some(description) = transfer.description() else {
                // 描述无效
                continue;
            };
            // 兼容：保留并登记
            if manifest.is_some_and(|manifest| compatible(&description, manifest)) {
                // 保留其路径
                retained.insert(description.path.clone());
                // 登记进行中的传输
                self.in_flight
                    // 按路径取条目
                    .entry(description.path.clone())
                    // 没有则插入
                    .or_insert(description);
            // 不兼容：取消
            } else {
                // 取消该传输
                transfer.cancel();
                // 移出进行中的传输
                self.in_flight.remove(&description.path);
            }
        }
        // 我们由系统持有的队列可能比本进程活得更久。裁剪之前先调和其文件。
        // 系统拥有的队列可能比本进程活得久；清理暂存文件前先对账
        self.in_flight
            // 只保留与清单兼容的传输
            .retain(|_, transfer| manifest.is_some_and(|manifest| compatible(transfer, manifest)));
        // 进行中的传输也要保留
        retained.extend(self.in_flight.keys().cloned());
        // 删除未被保留的暂存文件
        if let Ok(files) = fs::read_dir(self.root.join("staging")) {
            // 遍历暂存目录里的文件
            for file in files.flatten() {
                // 不在保留集合里
                if !retained.contains(&file.path()) {
                    // 删除
                    files::remove(&self.root, &file.path());
                }
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
