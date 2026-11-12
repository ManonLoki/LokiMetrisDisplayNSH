//! 一条 TCP 连接的帧边界与读写期限。
//! 连接、读取和写入各自阻塞在自己的线程上，结果按代次送回 owner 的事件队列；
//! owner 只在期限到达或事件到来时醒来，不轮询 socket。
use super::{Event, HANDSHAKE_FRAME, MAX_FRAME, err};
use serde_json::Value;
use std::{
    io::{ErrorKind, Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    sync::{
        // 原子引用计数与互斥锁
        Arc,
        // 互斥锁
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    // 线程
    thread,
    time::{Duration, Instant},
};

mod io;
use io::{open, read_frames};

// 读写超时 15 秒
const IO_TIMEOUT: Duration = Duration::from_secs(15);
// 发送队列最多 8 帧
const MAX_QUEUED: usize = 8;

#[derive(PartialEq, Clone, Copy)]
pub(super) enum Phase {
    // 配对第一步：等待桌面问候
    Hello,
    // 配对第二步：等待确认
    Confirmation,
    Resume,
    // 已建立加密通道，推送快照
    Streaming,
}

/// 线程送回 owner 的连接事实；旧代次的事件由 owner 丢弃。
// 线程送回 owner 的连接事实；旧代次的事件由 owner 丢弃
pub(super) enum SocketEvent {
    Frame(Value),
    /// 发送队列已全部写入内核；配对 ACK 与暂停通知据此收尾。
    // 发送队列已全部写入内核；配对 ACK 与暂停通知据此收尾
    Drained,
    Closed(String),
}

#[derive(Default)]
// 连接线程与 owner 共享的状态
struct Shared {
    stream: Option<TcpStream>,
    // 写线程的队列
    writer: Option<SyncSender<Vec<u8>>>,
    // 连接建立前排队的帧
    early: Vec<Vec<u8>>,
    closed: bool,
}

// 一条带帧边界的 TCP 连接
pub(super) struct FramedSocket {
    pub(super) generation: u64,
    pub(super) phase: Phase,
    pub(super) deadline: Instant,
    // 最近一次入站时间
    pub(super) last_inbound: Instant,
    /// 配对 ACK 已排队：写完即结束本次配对连接。
    // 配对 ACK 已排队：写完即结束本次配对连接
    pub(super) paired_close: bool,
    /// 暂停通知已排队：写完即关闭，不再处理后续数据。
    // 暂停通知已排队：写完即关闭，不再处理后续数据
    pub(super) pausing: bool,
    pub(super) peer: SocketAddr,
    shared: Arc<Mutex<Shared>>,
    // 排队中尚未写出的帧数
    pending: Arc<AtomicUsize>,
    // 是否已进入推送阶段（决定帧上限与是否逐帧等待）
    streaming: Arc<AtomicBool>,
    // 读线程的“可以继续读”许可
    permit: SyncSender<()>,
}

// 带帧的套接字的方法
impl FramedSocket {
    /// 在后台线程连接；连接前排队的帧在连上后按序写出。
    // 在后台线程连接；连接前排队的帧在连上后按序写出
    pub(super) fn connect(
        address: SocketAddr,
        pairing: bool,
        generation: u64,
        events: SyncSender<Event>,
    ) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let pending = Arc::new(AtomicUsize::new(0));
        let streaming = Arc::new(AtomicBool::new(false));
        let (permit, permits) = mpsc::sync_channel(1);
        // 线程使用的克隆
        let thread_shared = shared.clone();
        // 线程使用的计数副本
        let thread_pending = pending.clone();
        // 线程使用的流标记副本
        let thread_streaming = streaming.clone();
        // 启动读线程：先连接，再循环读帧，结束时通知 owner
        let spawned = thread::Builder::new()
            // 线程名
            .name("loki-lan-read".into())
            // 启动线程
            .spawn(move || {
                // 连接失败的错误或读循环结束的原因
                let reason = match open(
                    address,
                    &thread_shared,
                    &thread_pending,
                    generation,
                    &events,
                ) {
                    Ok(stream) => {
                        read_frames(stream, &thread_streaming, &permits, generation, &events)
                    }
                    // 读取出错则取错误作为关闭原因
                    Err(error) => error,
                };
                let _ = events.send(Event::Socket(generation, SocketEvent::Closed(reason)));
            });
        let now = Instant::now();
        // 初始阶段：配对从 Hello 开始，否则从 Resume 开始
        let socket = Self {
            generation,
            // 阶段：配对问候或恢复
            phase: if pairing { Phase::Hello } else { Phase::Resume },
            deadline: now + IO_TIMEOUT,
            // 最近收到数据的时刻
            last_inbound: now,
            paired_close: false,
            pausing: false,
            peer: address,
            shared,
            pending,
            streaming,
            permit,
        };
        // 线程创建失败则立即关闭
        if spawned.is_err() {
            socket.close();
        }
        socket
    }

    /// 握手阶段使用小帧上限；进入加密流后放宽到安全帧上限。
    // 握手阶段使用小帧上限；进入加密流后放宽到安全帧上限
    fn limit(&self) -> usize {
        if self.phase == Phase::Streaming {
            MAX_FRAME
        // 否则分支
        } else {
            HANDSHAKE_FRAME
        }
    }

    // 排队发送一个 JSON 值：加 4 字节大端长度前缀
    pub(super) fn send(&mut self, value: &Value) -> Result<(), String> {
        // 序列化为 JSON
        let bytes = serde_json::to_vec(value).map_err(err)?;
        if bytes.is_empty() || bytes.len() > self.limit() {
            return Err("网络发送队列或帧超出限制".into());
        }
        if self.pending.load(Ordering::SeqCst) >= MAX_QUEUED {
            return Err("网络发送队列或帧超出限制".into());
        }
        let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&bytes);
        let mut shared = self.shared.lock().unwrap_or_else(|p| p.into_inner());
        if shared.closed {
            return Err("连接已关闭".into());
        }
        self.pending.fetch_add(1, Ordering::SeqCst);
        // 已连接则交给写线程，否则先暂存
        match &shared.writer {
            // 已有写线程则交给它
            Some(writer) => writer
                .try_send(frame)
                // 失败报队列或帧超限
                .map_err(|_| "网络发送队列或帧超出限制".to_owned()),
            // 没有写线程
            None => {
                // 否则先放入早期队列
                shared.early.push(frame);
                Ok(())
            }
        }
    }

    /// 发送队列已经写空。
    pub(super) fn drained(&self) -> bool {
        // 待发送帧为 0 时视为已清空
        self.pending.load(Ordering::SeqCst) == 0
    }

    /// owner 处理完一帧握手消息后，才允许读线程读取下一帧。
    // owner 处理完一帧握手消息后，才允许读线程读取下一帧
    pub(super) fn advance(&self) {
        self.streaming
            // 记录是否进入传输阶段
            .store(self.phase == Phase::Streaming, Ordering::SeqCst);
        // 放行写线程
        let _ = self.permit.try_send(());
    }

    /// 关闭 socket，读写线程随之退出；重复调用无副作用。
    // 关闭 socket，读写线程随之退出；重复调用无副作用
    pub(super) fn close(&self) {
        let mut shared = self.shared.lock().unwrap_or_else(|p| p.into_inner());
        shared.closed = true;
        // 放弃写线程
        shared.writer = None;
        shared.early.clear();
        if let Some(stream) = shared.stream.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

impl Drop for FramedSocket {
    fn drop(&mut self) {
        self.close();
    }
}
