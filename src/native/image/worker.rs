//! 有界的解码工作线程让图片准备工作远离所有 NSH 引擎线程。
use std::{
    // Future 特征
    future::Future,
    pin::Pin,
    sync::{
        // 原子引用计数、互斥锁与一次性初始化
        Arc,
        // 互斥锁
        Mutex,
        // 一次性初始化
        OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
    },
    // 任务、上下文、轮询结果与唤醒器
    task::{Context, Poll, Waker},
};

/// 一个运行中的任务,以及最多 32 个等待中的任务;字节预算包含
/// 运行中的任务,直到其来源字节被释放为止。
// 一个运行中的任务加最多 32 个排队任务
const QUEUE_LENGTH: usize = 32;
// 字节预算（含运行中任务，直到其源字节被释放）
const QUEUED_BYTES: usize = 48 * 1024 * 1024;

#[derive(Default)]
// 完成状态：结果与唤醒器
struct Completion {
    result: Option<Result<String, String>>,
    waker: Option<Waker>,
}

#[derive(Default)]
// 提交方与工作线程共享的状态
struct Shared {
    // 提交方是否已放弃等待
    cancelled: AtomicBool,
    completion: Mutex<Completion>,
}

// 提交方持有的 Future
struct Pending(Arc<Shared>);

// 未完成任务的 Future 实现
impl Future for Pending {
    type Output = Result<String, String>;

    // 轮询：有结果则返回，否则登记唤醒器
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let next_waker = context.waker().clone();
        // 登记与完成共用同一把锁；自定义唤醒器的克隆与释放始终在锁外执行
        let (result, previous_waker) = {
            let mut completion = self.0.completion.lock().unwrap_or_else(|p| p.into_inner());
            // 已有结果：交出结果并取走旧唤醒器
            if let Some(result) = completion.result.take() {
                // 就绪：返回结果并取走旧唤醒器
                (Poll::Ready(result), completion.waker.take())
            // 没有结果：登记新唤醒器，旧的稍后在锁外释放
            } else {
                // 注册与完成共用同一把锁,而自定义的
                // waker 克隆/释放代码总是在锁之外运行。
                (Poll::Pending, completion.waker.replace(next_waker))
            }
        };
        drop(previous_waker);
        result
    }
}

// 提交方被丢弃：标记取消并释放唤醒器
impl Drop for Pending {
    fn drop(&mut self) {
        self.0.cancelled.store(true, Ordering::Release);
        let waker = self
            .0
            .completion
            // 加锁
            .lock()
            // 锁中毒时仍取回内部状态，不让一次 panic 连带拖垮其他线程
            .unwrap_or_else(|p| p.into_inner())
            .waker
            .take();
        // 在锁外释放
        drop(waker);
    }
}

// 字节预算的占用凭证，释放时归还
struct Reservation {
    bytes: usize,
    budget: Arc<AtomicUsize>,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

struct Job {
    source: String,
    shared: Arc<Shared>,
    // 任务结束时自动归还预算
    _reservation: Reservation,
}

// 解码服务：队列发送端与字节预算
struct Service {
    sender: SyncSender<Job>,
    budget: Arc<AtomicUsize>,
}

impl Service {
    // 启动单个工作线程，按序处理任务
    fn start() -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_LENGTH);
        // 创建工作线程
        std::thread::Builder::new()
            // 线程名
            .name("loki-gif-frames".into())
            // 启动线程
            .spawn(move || {
                for job in receiver {
                    process(job, super::prepare);
                }
            })
            // 线程无法启动
            .map_err(|_| "GIF解码线程无法启动")?;
        Ok(Self {
            sender,
            budget: Arc::default(),
        })
    }

    // 提交任务：先检查来源长度和字节预算，再入队
    fn submit(&self, source: String) -> Result<Pending, String> {
        let bytes = source.len();
        // 单个来源的长度上限
        if bytes > super::MAX_SOURCE.div_ceil(3) * 4 + 64 {
            return Err("GIF来源超过长度限额".into());
        }
        // 占用预算，超出则拒绝
        self.budget
            // 原子地更新已用额度
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                // 已用额度加上本次不得超过队列上限
                used.checked_add(bytes).filter(|next| *next <= QUEUED_BYTES)
            })
            .map_err(|_| "GIF解码队列字节限额已满")?;
        let shared = Arc::new(Shared::default());
        let job = Job {
            source,
            shared: Arc::clone(&shared),
            _reservation: Reservation {
                bytes,
                budget: Arc::clone(&self.budget),
            },
        };
        // 队列满则拒绝（凭证随任务释放，预算自动归还）
        // 入队，失败则任务被丢弃、预算随之归还
        self.sender
            .try_send(job)
            // 队列已满或线程不可用
            .map_err(|_| "GIF解码队列已满或线程不可用")?;
        Ok(Pending(shared))
    }
}

// 工作线程处理任务：已取消则跳过，完成后唤醒等待方
fn process(job: Job, prepare: impl FnOnce(&str) -> Result<String, String>) {
    if job.shared.cancelled.load(Ordering::Acquire) {
        return;
    }
    let result = prepare(&job.source);
    let waker = {
        let mut completion = job
            .shared
            .completion
            // 加锁
            .lock()
            // 锁中毒时仍取回内部状态，不让一次 panic 连带拖垮其他线程
            .unwrap_or_else(|p| p.into_inner());
        // 解码期间被取消则丢弃结果
        if job.shared.cancelled.load(Ordering::Acquire) {
            return;
        }
        completion.result = Some(result);
        completion.waker.take()
    };
    // 在锁外唤醒
    if let Some(waker) = waker {
        // 在锁外唤醒
        waker.wake();
    }
}

// 对外入口：懒启动服务并提交任务
pub(super) async fn prepare(source: String) -> Result<String, String> {
    static SERVICE: OnceLock<Result<Service, String>> = OnceLock::new();
    let service = SERVICE
        // 首次使用时启动服务
        .get_or_init(Service::start)
        .as_ref()
        .map_err(Clone::clone)?;
    service.submit(source)?.await
}

#[cfg(test)]
#[path = "worker_test.rs"]
mod worker_test;
