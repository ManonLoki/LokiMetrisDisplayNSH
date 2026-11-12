use super::*;

impl Owner {
    // 最近的期限：配对窗口、重连、套接字、资源传输与发现退避中最早的一个
    pub(super) fn next_deadline(&self) -> Option<Instant> {
        // 推送阶段还要考虑空闲超时
        let socket = self.socket.as_ref().map(|w| {
            if w.phase == Phase::Streaming && !w.pausing {
                // 取期限与空闲超时中较早者
                w.deadline.min(w.last_inbound + IDLE_TIMEOUT)
            // 否则分支
            } else {
                // 否则使用期限
                w.deadline
            }
        });
        [
            self.pairing_deadline,
            self.retry_at,
            socket,
            self.assets.deadline(),
            self.discovery.retry_at(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub(super) fn expire(&mut self, now: Instant) {
        // 配对窗口过期：取消并提示
        if self.pairing_deadline.is_some_and(|d| now >= d) {
            self.cancel();
            self.view["status"] = json!("暂未连接");
            // 错误：配对窗口已过期
            self.view["error"] = json!("配对窗口已过期，请重新选择桌面。");
            self.publish();
        }
        if self.assets.deadline().is_some_and(|d| now >= d) {
            self.asset_timeout();
        }
        // 发现退避结束：重新启动发现
        if self.discovery.retry_at().is_some_and(|d| now >= d) {
            self.discovery.clear_retry();
            self.update_discovery();
        }
        if let Some(socket) = &self.socket {
            // 超过期限：暂停握手中的直接断开，其余视为超时失败
            if now >= socket.deadline {
                if socket.pausing {
                    self.drop_socket();
                // 否则
                } else {
                    self.failed("连接超时，正在重试…".into());
                }
            // 推送阶段长时间没有入站数据
            } else if socket.phase == Phase::Streaming
                && !socket.pausing
                // 并且空闲时间超过上限
                && now.duration_since(socket.last_inbound) >= IDLE_TIMEOUT
            {
                self.failed("桌面暂不可达，恢复网络后重新连接。".into());
            }
        }
        // 到了重连时间：发起连接，失败则继续退避
        if self.retry_at.is_some_and(|t| now >= t) {
            self.retry_at = None;
            if let Err(e) = self.connect() {
                self.failed(e);
            }
        }
    }

    pub(super) fn socket_event(&mut self, event: SocketEvent) {
        match event {
            SocketEvent::Frame(value) => {
                // 记录入站时间；暂停握手期间丢弃内容
                if let Some(socket) = self.socket.as_mut() {
                    // 记录最近一次收到数据的时刻
                    socket.last_inbound = Instant::now();
                    if socket.pausing {
                        return;
                    }
                }
                // 处理失败则断开并重试
                if let Err(e) = self.received(value) {
                    self.failed(e);
                    return;
                }
                // 处理成功后继续读取下一帧
                if let Some(socket) = &self.socket {
                    socket.advance();
                }
            }
            SocketEvent::Drained => self.drained(),
            SocketEvent::Closed(reason) => {
                let Some(socket) = &self.socket else { return };
                // 配对确认已发完：配对完成
                if socket.paired_close && socket.drained() {
                    self.pairing_completed();
                } else if socket.pausing {
                    self.drop_socket();
                // 否则视为失败
                } else {
                    self.failed(reason);
                }
            }
        }
    }

    // 发送队列清空后的收尾
    pub(super) fn drained(&mut self) {
        let Some(socket) = &self.socket else { return };
        if socket.paired_close {
            self.pairing_completed();
        } else if socket.pausing {
            self.drop_socket();
        }
    }

    /// 保存 ACK 写完即结束配对连接，稍后以新 TCP 和已存秘密认证。
    // 保存确认写完即结束配对连接，稍后用新的 TCP 与已保存的秘密认证
    pub(super) fn pairing_completed(&mut self) {
        self.drop_socket();
        self.clear_pairing();
        // 500 毫秒后重连
        self.retry_at = Some(Instant::now() + Duration::from_millis(500));
        self.publish();
    }
}
