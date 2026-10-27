//! 一条 TCP 连接的帧边界与读写期限。
//! 连接、读取和写入各自阻塞在自己的线程上，结果按代次送回 owner 的事件队列；
//! owner 只在期限到达或事件到来时醒来，不轮询 socket。
use super::{Event, HANDSHAKE_FRAME, MAX_FRAME, err};
use serde_json::Value;
use std::{
    io::{ErrorKind, Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

const IO_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_QUEUED: usize = 8;

#[derive(PartialEq, Clone, Copy)]
pub(super) enum Phase {
    Hello,
    Confirmation,
    Resume,
    Streaming,
}

/// 线程送回 owner 的连接事实；旧代次的事件由 owner 丢弃。
pub(super) enum WireEvent {
    Frame(Value),
    /// 发送队列已全部写入内核；配对 ACK 与暂停通知据此收尾。
    Drained,
    Closed(String),
}

#[derive(Default)]
struct Shared {
    stream: Option<TcpStream>,
    writer: Option<SyncSender<Vec<u8>>>,
    early: Vec<Vec<u8>>,
    closed: bool,
}

pub(super) struct Wire {
    pub(super) generation: u64,
    pub(super) phase: Phase,
    pub(super) deadline: Instant,
    pub(super) last_inbound: Instant,
    /// 配对 ACK 已排队：写完即结束本次配对连接。
    pub(super) paired_close: bool,
    /// 暂停通知已排队：写完即关闭，不再处理后续数据。
    pub(super) pausing: bool,
    pub(super) peer: SocketAddr,
    shared: Arc<Mutex<Shared>>,
    pending: Arc<AtomicUsize>,
    streaming: Arc<AtomicBool>,
    permit: SyncSender<()>,
}

impl Wire {
    /// 在后台线程连接；连接前排队的帧在连上后按序写出。
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
        let thread_shared = shared.clone();
        let thread_pending = pending.clone();
        let thread_streaming = streaming.clone();
        let spawned = thread::Builder::new()
            .name("loki-lan-read".into())
            .spawn(move || {
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
                    Err(error) => error,
                };
                let _ = events.send(Event::Wire(generation, WireEvent::Closed(reason)));
            });
        let now = Instant::now();
        let wire = Self {
            generation,
            phase: if pairing { Phase::Hello } else { Phase::Resume },
            deadline: now + IO_TIMEOUT,
            last_inbound: now,
            paired_close: false,
            pausing: false,
            peer: address,
            shared,
            pending,
            streaming,
            permit,
        };
        if spawned.is_err() {
            wire.close();
        }
        wire
    }

    /// 握手阶段使用小帧上限；进入加密流后放宽到安全帧上限。
    fn limit(&self) -> usize {
        if self.phase == Phase::Streaming {
            MAX_FRAME
        } else {
            HANDSHAKE_FRAME
        }
    }

    pub(super) fn send(&mut self, value: &Value) -> Result<(), String> {
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
        match &shared.writer {
            Some(writer) => writer
                .try_send(frame)
                .map_err(|_| "网络发送队列或帧超出限制".to_owned()),
            None => {
                shared.early.push(frame);
                Ok(())
            }
        }
    }

    /// 发送队列已经写空。
    pub(super) fn drained(&self) -> bool {
        self.pending.load(Ordering::SeqCst) == 0
    }

    /// owner 处理完一帧握手消息后，才允许读线程读取下一帧。
    pub(super) fn advance(&self) {
        self.streaming
            .store(self.phase == Phase::Streaming, Ordering::SeqCst);
        let _ = self.permit.try_send(());
    }

    /// 关闭 socket，读写线程随之退出；重复调用无副作用。
    pub(super) fn close(&self) {
        let mut shared = self.shared.lock().unwrap_or_else(|p| p.into_inner());
        shared.closed = true;
        shared.writer = None;
        shared.early.clear();
        if let Some(stream) = shared.stream.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

impl Drop for Wire {
    fn drop(&mut self) {
        self.close();
    }
}

fn open(
    address: SocketAddr,
    shared: &Mutex<Shared>,
    pending: &Arc<AtomicUsize>,
    generation: u64,
    events: &SyncSender<Event>,
) -> Result<TcpStream, String> {
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(3)).map_err(err)?;
    stream.set_nodelay(true).map_err(err)?;
    stream.set_write_timeout(Some(IO_TIMEOUT)).map_err(err)?;
    let writer_stream = stream.try_clone().map_err(err)?;
    let (writer, frames) = mpsc::sync_channel(MAX_QUEUED);
    let mut state = shared.lock().unwrap_or_else(|p| p.into_inner());
    if state.closed {
        let _ = stream.shutdown(Shutdown::Both);
        return Err("连接已关闭".into());
    }
    for frame in state.early.drain(..) {
        writer
            .try_send(frame)
            .map_err(|_| "网络发送队列或帧超出限制")?;
    }
    state.stream = Some(stream.try_clone().map_err(err)?);
    state.writer = Some(writer);
    drop(state);
    let pending = pending.clone();
    let events = events.clone();
    thread::Builder::new()
        .name("loki-lan-write".into())
        .spawn(move || write_frames(writer_stream, frames, &pending, generation, &events))
        .map_err(err)?;
    Ok(stream)
}

fn write_frames(
    mut stream: TcpStream,
    frames: Receiver<Vec<u8>>,
    pending: &AtomicUsize,
    generation: u64,
    events: &SyncSender<Event>,
) {
    for frame in frames {
        if let Err(error) = stream.write_all(&frame) {
            let reason = if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) {
                "网络帧发送超时".into()
            } else {
                err(error)
            };
            let _ = stream.shutdown(Shutdown::Both);
            let _ = events.send(Event::Wire(generation, WireEvent::Closed(reason)));
            return;
        }
        if pending.fetch_sub(1, Ordering::SeqCst) == 1 {
            let _ = events.send(Event::Wire(generation, WireEvent::Drained));
        }
    }
}

/// 先校验声明长度再分配正文；握手阶段每帧都等 owner 推进阶段后才继续读取，
/// 未认证的对端不能提前发送后续阶段的数据。
fn read_frames(
    mut stream: TcpStream,
    streaming: &AtomicBool,
    permits: &Receiver<()>,
    generation: u64,
    events: &SyncSender<Event>,
) -> String {
    let mut buffer = Vec::new();
    let mut partial_since: Option<Instant> = None;
    let mut chunk = [0u8; 8192];
    loop {
        let limit = if streaming.load(Ordering::SeqCst) {
            MAX_FRAME
        } else {
            HANDSHAKE_FRAME
        };
        while buffer.len() >= 4 {
            let size = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
            if size == 0 || size > limit {
                return "无效网络帧长度".into();
            }
            if buffer.len() < size + 4 {
                break;
            }
            let value: Value = match serde_json::from_slice(&buffer[4..4 + size]) {
                Ok(value) => value,
                Err(error) => return err(error),
            };
            buffer.drain(..4 + size);
            partial_since = (!buffer.is_empty()).then(Instant::now);
            let handshake = !streaming.load(Ordering::SeqCst);
            if handshake && !buffer.is_empty() {
                return "握手阶段提前发送数据".into();
            }
            if events
                .send(Event::Wire(generation, WireEvent::Frame(value)))
                .is_err()
            {
                return "连接已关闭".into();
            }
            if handshake && permits.recv().is_err() {
                return "连接已关闭".into();
            }
        }
        if buffer.len() > limit + 4 {
            return "网络帧超出限制".into();
        }
        let timeout = partial_since.map(|since| {
            IO_TIMEOUT
                .checked_sub(since.elapsed())
                .unwrap_or(Duration::from_millis(1))
                .max(Duration::from_millis(1))
        });
        if stream.set_read_timeout(timeout).is_err() {
            return "连接已关闭".into();
        }
        match stream.read(&mut chunk) {
            Ok(0) => return "连接已关闭".into(),
            Ok(n) => {
                if buffer.is_empty() {
                    partial_since = Some(Instant::now());
                }
                buffer.extend_from_slice(&chunk[..n]);
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return "网络帧接收超时".into();
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return err(e),
        }
    }
}
