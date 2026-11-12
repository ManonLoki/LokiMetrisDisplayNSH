use super::*;

// 建立 TCP 连接并启动写线程
pub(super) fn open(
    address: SocketAddr,
    shared: &Mutex<Shared>,
    pending: &Arc<AtomicUsize>,
    generation: u64,
    events: &SyncSender<Event>,
) -> Result<TcpStream, String> {
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(3)).map_err(err)?;
    // 关闭 Nagle 算法，减少延迟
    stream.set_nodelay(true).map_err(err)?;
    stream.set_write_timeout(Some(IO_TIMEOUT)).map_err(err)?;
    // 写线程用的克隆
    let writer_stream = stream.try_clone().map_err(err)?;
    let (writer, frames) = mpsc::sync_channel(MAX_QUEUED);
    let mut state = shared.lock().unwrap_or_else(|p| p.into_inner());
    if state.closed {
        let _ = stream.shutdown(Shutdown::Both);
        return Err("连接已关闭".into());
    }
    // 把连接前排队的帧按序交给写线程
    for frame in state.early.drain(..) {
        // 把早期队列里的帧送进写通道
        writer
            .try_send(frame)
            // 失败报队列或帧超限
            .map_err(|_| "网络发送队列或帧超出限制")?;
    }
    state.stream = Some(stream.try_clone().map_err(err)?);
    state.writer = Some(writer);
    // 释放锁
    drop(state);
    let pending = pending.clone();
    let events = events.clone();
    // 启动写线程
    thread::Builder::new()
        // 线程名
        .name("loki-lan-write".into())
        // 启动写线程
        .spawn(move || write_frames(writer_stream, frames, &pending, generation, &events))
        .map_err(err)?;
    Ok(stream)
}

// 写线程：依次写出帧，队列写空时通知 owner
pub(super) fn write_frames(
    mut stream: TcpStream,
    frames: Receiver<Vec<u8>>,
    pending: &AtomicUsize,
    generation: u64,
    events: &SyncSender<Event>,
) {
    for frame in frames {
        // 写失败：区分超时与其他错误并通知关闭
        if let Err(error) = stream.write_all(&frame) {
            // 写超时则报网络帧发送超时
            let reason = if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) {
                "网络帧发送超时".into()
            // 否则分支
            } else {
                err(error)
            };
            let _ = stream.shutdown(Shutdown::Both);
            let _ = events.send(Event::Socket(generation, SocketEvent::Closed(reason)));
            return;
        }
        // 最后一帧写完时通知 Drained
        if pending.fetch_sub(1, Ordering::SeqCst) == 1 {
            let _ = events.send(Event::Socket(generation, SocketEvent::Drained));
        }
    }
}

/// 先校验声明长度再分配正文；握手阶段每帧都等 owner 推进阶段后才继续读取，
/// 未认证的对端不能提前发送后续阶段的数据。
// 先校验声明长度再分配正文；握手阶段每帧都等 owner 推进阶段后才继续读取，未认证的对端不能提前发送后续阶段的数据
pub(super) fn read_frames(
    mut stream: TcpStream,
    streaming: &AtomicBool,
    permits: &Receiver<()>,
    generation: u64,
    events: &SyncSender<Event>,
) -> String {
    let mut buffer = Vec::new();
    // 一帧开始接收的时间，用于不完整帧的超时
    let mut partial_since: Option<Instant> = None;
    let mut chunk = [0u8; 8192];
    loop {
        let limit = if streaming.load(Ordering::SeqCst) {
            MAX_FRAME
        // 否则分支
        } else {
            HANDSHAKE_FRAME
        };
        // 缓冲区中有完整长度前缀时解析帧
        while buffer.len() >= 4 {
            // 由前四字节得到帧长度
            let size = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
            if size == 0 || size > limit {
                return "无效网络帧长度".into();
            }
            if buffer.len() < size + 4 {
                break;
            }
            let value: Value = match serde_json::from_slice(&buffer[4..4 + size]) {
                Ok(value) => value,
                // 出错则返回错误原因
                Err(error) => return err(error),
            };
            buffer.drain(..4 + size);
            // 缓冲区仍有数据则重新计时
            partial_since = (!buffer.is_empty()).then(Instant::now);
            let handshake = !streaming.load(Ordering::SeqCst);
            // 握手阶段不允许对端提前发送后续数据
            if handshake && !buffer.is_empty() {
                // 握手阶段提前发送数据
                return "握手阶段提前发送数据".into();
            }
            // 把帧交给 owner
            if events
                .send(Event::Socket(generation, SocketEvent::Frame(value)))
                .is_err()
            {
                return "连接已关闭".into();
            }
            // 握手阶段等 owner 处理完才继续读
            if handshake && permits.recv().is_err() {
                return "连接已关闭".into();
            }
        }
        if buffer.len() > limit + 4 {
            return "网络帧超出限制".into();
        }
        // 有不完整帧时设置剩余的读超时
        let timeout = partial_since.map(|since| {
            IO_TIMEOUT
                .checked_sub(since.elapsed())
                // 不足则取 1 毫秒
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
                    // 记下开始接收不完整帧的时刻
                    partial_since = Some(Instant::now());
                }
                buffer.extend_from_slice(&chunk[..n]);
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return "网络帧接收超时".into();
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            // 读取出错，返回错误原因
            Err(e) => return err(e),
        }
    }
}
