use super::*;

impl Owner {
    // 事件循环：事件到来或最近期限到达才醒来，空闲、暂停、后台时不周期唤醒
    pub(in crate::native::connection) fn run(&mut self, rx: Receiver<Event>) {
        // 主循环
        loop {
            // 句柄释放后退出
            if self.stopped.load(Ordering::SeqCst) {
                // 结束循环
                break;
            }
            // 提交上一轮的变化
            self.commit();
            // 当前时刻
            let now = Instant::now();
            // 等到最近的期限，没有期限则最长等一小时
            let timeout = self
                // 下一个期限
                .next_deadline()
                // 换算为等待时长，没有期限则等一小时
                .map_or(Duration::from_secs(3600), |at| {
                    // 距最近期限的等待时间
                    at.saturating_duration_since(now)
                });
            // 等待命令或超时
            match rx.recv_timeout(timeout) {
                // 收到停止命令或发送端全部断开：退出
                Ok(Event::Command(QueuedCommand {
                    // 通道超时：当作停止命令
                    command: Command::Stop,
                    // 其余字段忽略
                    ..
                }))
                // 通道断开也跳出循环
                | Err(RecvTimeoutError::Disconnected) => break,
                // 普通命令：失败则展示错误
                Ok(Event::Command(command)) => {
                    // 应用命令，失败则
                    if let Err(e) = self.apply(command) {
                        // 把错误写入视图
                        self.view["error"] = json!(e);
                        // 发布视图
                        self.publish();
                    }
                }
                // 套接字事件：只处理当前代数的连接
                Ok(Event::Socket(generation, event)) => {
                    // 判断是否应退出
                    if self
                        // 套接字
                        .socket
                        // 取引用
                        .as_ref()
                        // 代次一致
                        .is_some_and(|w| w.generation == generation)
                    {
                        // 处理套接字事件
                        self.socket_event(event);
                    }
                }
                // 发现到桌面
                Ok(Event::Discovery(found)) => self.found(found),
                // 超时只为检查期限
                Err(RecvTimeoutError::Timeout) => {}
            }
            // 配对的取消代数已变化：取消配对
            if self
                // 协议
                .protocol
                // 配对修订号
                .pairing_revision()
                // 与当前修订号不同
                .is_some_and(|r| r != self.revision.load(Ordering::SeqCst))
            {
                // 取消配对
                self.cancel();
                // 状态：配对已取消
                self.view["status"] = json!("配对已取消，请重新选择桌面。");
                // 发布视图
                self.publish();
            }
            // 处理所有到期的期限
            self.expire(Instant::now());
        }
        // 退出前清理并提交最后的状态
        self.cancel();
        // 停止发现
        self.discovery.stop();
        // 提交变更
        self.commit();
    }
}
