use super::*;

impl Owner {
    // 事件循环：事件到来或最近期限到达才醒来，空闲、暂停、后台时不周期唤醒
    pub(in crate::native::connection) fn run(&mut self, rx: Receiver<Event>) {
        loop {
            if self.stopped.load(Ordering::SeqCst) {
                break;
            }
            self.commit();
            let now = Instant::now();
            // 等到最近的期限，没有期限则最长等一小时
            let timeout = self
                .next_deadline()
                // 换算为等待时长，没有期限则等一小时
                .map_or(Duration::from_secs(3600), |at| {
                    // 距最近期限的等待时间
                    at.saturating_duration_since(now)
                });
            match rx.recv_timeout(timeout) {
                // 收到停止命令或发送端全部断开：退出
                Ok(Event::Command(QueuedCommand {
                    // 通道超时：当作停止命令
                    command: Command::Stop,
                    ..
                }))
                // 通道断开也跳出循环
                | Err(RecvTimeoutError::Disconnected) => break,
                // 普通命令：失败则展示错误
                Ok(Event::Command(command)) => {
                    if let Err(e) = self.apply(command) {
                        self.view["error"] = json!(e);
                        self.publish();
                    }
                }
                // 套接字事件：只处理当前代数的连接
                Ok(Event::Socket(generation, event)) => {
                    if self
                        .socket
                        .as_ref()
                        .is_some_and(|w| w.generation == generation)
                    {
                        self.socket_event(event);
                    }
                }
                Ok(Event::Discovery(found)) => self.found(found),
                #[cfg(any(target_os = "ios", test))]
                Ok(Event::Companion { generation, scope, nonce, report }) => self.watch_report(generation, &scope, nonce, report),
                // 超时只为检查期限
                Err(RecvTimeoutError::Timeout) => {}
            }
            // 配对的取消代数已变化：取消配对
            if self
                .protocol
                .pairing_revision()
                .is_some_and(|r| r != self.revision.load(Ordering::SeqCst))
            {
                self.cancel();
                self.view["status"] = json!("配对已取消，请重新选择桌面。");
                self.publish();
            }
            // 处理所有到期的期限
            self.expire(Instant::now());
        }
        // 退出前清理并提交最后的状态
        self.cancel();
        self.discovery.stop();
        self.commit();
    }
}
