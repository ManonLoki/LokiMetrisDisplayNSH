use super::*;

// 建立 TCP 连接并启动写线程
pub(super) fn open(
    // 地址
    address: SocketAddr,
    // 共享状态
    shared: &Mutex<Shared>,
    // 待发送计数
    pending: &Arc<AtomicUsize>,
    // 连接代次
    generation: u64,
    // 事件发送端
    events: &SyncSender<Event>,
    // 返回流
) -> Result<TcpStream, String> {
    // 连接超时 3 秒
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(3)).map_err(err)?;
    // 关闭 Nagle 算法，减少延迟
    stream.set_nodelay(true).map_err(err)?;
    // 设置写超时
    stream.set_write_timeout(Some(IO_TIMEOUT)).map_err(err)?;
    // 写线程用的克隆
    let writer_stream = stream.try_clone().map_err(err)?;
    // 创建发送通道
    let (writer, frames) = mpsc::sync_channel(MAX_QUEUED);
    // 取共享状态
    let mut state = shared.lock().unwrap_or_else(|p| p.into_inner());
    // 连接期间已被关闭
    if state.closed {
        // 关闭流
        let _ = stream.shutdown(Shutdown::Both);
        // 报连接已关闭
        return Err("连接已关闭".into());
    }
    // 把连接前排队的帧按序交给写线程
    for frame in state.early.drain(..) {
        // 把早期队列里的帧送进写通道
        writer
            // 非阻塞发送帧
            .try_send(frame)
            // 失败报队列或帧超限
            .map_err(|_| "网络发送队列或帧超出限制")?;
    }
    // 保存流副本
    state.stream = Some(stream.try_clone().map_err(err)?);
    // 保存写通道
    state.writer = Some(writer);
    // 释放锁
    drop(state);
    // 克隆待发队列
    let pending = pending.clone();
    // 克隆事件发送端
    let events = events.clone();
    // 启动写线程
    thread::Builder::new()
        // 线程名
        .name("loki-lan-write".into())
        // 启动写线程
        .spawn(move || write_frames(writer_stream, frames, &pending, generation, &events))
        // 失败转为错误
        .map_err(err)?;
    // 返回流
    Ok(stream)
}

// 写线程：依次写出帧，队列写空时通知 owner
pub(super) fn write_frames(
    // 可变的流
    mut stream: TcpStream,
    // 帧接收端
    frames: Receiver<Vec<u8>>,
    // 待发送计数
    pending: &AtomicUsize,
    // 连接代次
    generation: u64,
    // 事件发送端
    events: &SyncSender<Event>,
    // 函数体
) {
    // 逐帧写出
    for frame in frames {
        // 写失败：区分超时与其他错误并通知关闭
        if let Err(error) = stream.write_all(&frame) {
            // 写超时则报网络帧发送超时
            let reason = if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) {
                // 超时原因
                "网络帧发送超时".into()
            // 否则分支
            } else {
                // 其他错误
                err(error)
            };
            // 关闭流
            let _ = stream.shutdown(Shutdown::Both);
            // 通知已关闭
            let _ = events.send(Event::Socket(generation, SocketEvent::Closed(reason)));
            // 然后返回
            return;
        }
        // 最后一帧写完时通知 Drained
        if pending.fetch_sub(1, Ordering::SeqCst) == 1 {
            // 通知已发完
            let _ = events.send(Event::Socket(generation, SocketEvent::Drained));
        }
    }
}

/// 先校验声明长度再分配正文；握手阶段每帧都等 owner 推进阶段后才继续读取，
/// 未认证的对端不能提前发送后续阶段的数据。
// 先校验声明长度再分配正文；握手阶段每帧都等 owner 推进阶段后才继续读取，未认证的对端不能提前发送后续阶段的数据
pub(super) fn read_frames(
    // 可变的流
    mut stream: TcpStream,
    // 是否进入传输阶段
    streaming: &AtomicBool,
    // 放行许可接收端
    permits: &Receiver<()>,
    // 连接代次
    generation: u64,
    // 事件发送端
    events: &SyncSender<Event>,
    // 返回关闭原因
) -> String {
    // 接收缓冲区
    let mut buffer = Vec::new();
    // 一帧开始接收的时间，用于不完整帧的超时
    let mut partial_since: Option<Instant> = None;
    // 读缓冲区
    let mut chunk = [0u8; 8192];
    // 循环读取
    loop {
        // 当前阶段的帧上限
        let limit = if streaming.load(Ordering::SeqCst) {
            // 传输阶段的帧上限
            MAX_FRAME
        // 否则分支
        } else {
            // 握手阶段的帧上限
            HANDSHAKE_FRAME
        };
        // 缓冲区中有完整长度前缀时解析帧
        while buffer.len() >= 4 {
            // 由前四字节得到帧长度
            let size = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
            // 长度非法
            if size == 0 || size > limit {
                // 长度无效
                return "无效网络帧长度".into();
            }
            // 正文还没收全
            if buffer.len() < size + 4 {
                // 跳出内层循环
                break;
            }
            // 解析 JSON
            let value: Value = match serde_json::from_slice(&buffer[4..4 + size]) {
                // 帧解析成功
                Ok(value) => value,
                // 出错则返回错误原因
                Err(error) => return err(error),
            };
            // 移除已消费的帧
            buffer.drain(..4 + size);
            // 缓冲区仍有数据则重新计时
            partial_since = (!buffer.is_empty()).then(Instant::now);
            // 是否处于握手阶段
            let handshake = !streaming.load(Ordering::SeqCst);
            // 握手阶段不允许对端提前发送后续数据
            if handshake && !buffer.is_empty() {
                // 握手阶段提前发送数据
                return "握手阶段提前发送数据".into();
            }
            // 把帧交给 owner
            if events
                // 发送收到的帧
                .send(Event::Socket(generation, SocketEvent::Frame(value)))
                // 发送失败
                .is_err()
            {
                // 连接已关闭
                return "连接已关闭".into();
            }
            // 握手阶段等 owner 处理完才继续读
            if handshake && permits.recv().is_err() {
                // 连接已关闭
                return "连接已关闭".into();
            }
        }
        // 缓冲区超限
        if buffer.len() > limit + 4 {
            // 帧超出限制
            return "网络帧超出限制".into();
        }
        // 有不完整帧时设置剩余的读超时
        let timeout = partial_since.map(|since| {
            // 使用 IO 超时
            IO_TIMEOUT
                // 减去已等待的时间
                .checked_sub(since.elapsed())
                // 不足则取 1 毫秒
                .unwrap_or(Duration::from_millis(1))
                // 至少 1 毫秒
                .max(Duration::from_millis(1))
        });
        // 设置读超时失败
        if stream.set_read_timeout(timeout).is_err() {
            // 连接已关闭
            return "连接已关闭".into();
        }
        // 读取更多数据
        match stream.read(&mut chunk) {
            // 对端关闭
            Ok(0) => return "连接已关闭".into(),
            // 读到数据
            Ok(n) => {
                // 缓冲区为空
                if buffer.is_empty() {
                    // 记下开始接收不完整帧的时刻
                    partial_since = Some(Instant::now());
                }
                // 追加读到的字节
                buffer.extend_from_slice(&chunk[..n]);
            }
            // 不完整帧超时
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                // 不完整帧接收超时
                return "网络帧接收超时".into();
            }
            // 被信号打断则重试
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            // 读取出错，返回错误原因
            Err(e) => return err(e),
        }
    }
}
