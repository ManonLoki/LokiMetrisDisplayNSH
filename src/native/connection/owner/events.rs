use super::*;

impl Owner {
    // 最近的期限：配对窗口、重连、套接字、资源传输与发现退避中最早的一个
    pub(super) fn next_deadline(&self) -> Option<Instant> {
        // 推送阶段还要考虑空闲超时
        let socket = self.socket.as_ref().map(|w| {
            // 传输中且未暂停
            if w.phase == Phase::Streaming && !w.pausing {
                // 取期限与空闲超时中较早者
                w.deadline.min(w.last_inbound + IDLE_TIMEOUT)
            // 否则分支
            } else {
                // 否则使用期限
                w.deadline
            }
        });
        // 各类期限数组
        [
            // 配对期限
            self.pairing_deadline,
            // 重试时刻
            self.retry_at,
            // 套接字期限
            socket,
            // 资源期限
            self.assets.deadline(),
            // 发现重试时刻
            self.discovery.retry_at(),
        ]
        // 遍历存在的期限
        .into_iter()
        // 展平
        .flatten()
        // 取最早的
        .min()
    }

    // 处理已到期的期限
    pub(super) fn expire(&mut self, now: Instant) {
        // 配对窗口过期：取消并提示
        if self.pairing_deadline.is_some_and(|d| now >= d) {
            // 取消配对
            self.cancel();
            // 状态：暂未连接
            self.view["status"] = json!("暂未连接");
            // 错误：配对窗口已过期
            self.view["error"] = json!("配对窗口已过期，请重新选择桌面。");
            // 发布视图
            self.publish();
        }
        // 资源传输超时
        if self.assets.deadline().is_some_and(|d| now >= d) {
            // 处理资源超时
            self.asset_timeout();
        }
        // 发现退避结束：重新启动发现
        if self.discovery.retry_at().is_some_and(|d| now >= d) {
            // 清除发现重试
            self.discovery.clear_retry();
            // 更新发现状态
            self.update_discovery();
        }
        // 套接字期限
        if let Some(socket) = &self.socket {
            // 超过期限：暂停握手中的直接断开，其余视为超时失败
            if now >= socket.deadline {
                // 已暂停
                if socket.pausing {
                    // 丢弃套接字
                    self.drop_socket();
                // 否则
                } else {
                    // 报连接超时并重试
                    self.failed("连接超时，正在重试…".into());
                }
            // 推送阶段长时间没有入站数据
            } else if socket.phase == Phase::Streaming
                // 并且不在暂停中
                && !socket.pausing
                // 并且空闲时间超过上限
                && now.duration_since(socket.last_inbound) >= IDLE_TIMEOUT
            {
                // 桌面暂不可达
                self.failed("桌面暂不可达，恢复网络后重新连接。".into());
            }
        }
        // 到了重连时间：发起连接，失败则继续退避
        if self.retry_at.is_some_and(|t| now >= t) {
            // 清除重试时刻
            self.retry_at = None;
            // 重新连接
            if let Err(e) = self.connect() {
                // 失败则记录
                self.failed(e);
            }
        }
    }

    // 处理套接字事件
    pub(super) fn socket_event(&mut self, event: SocketEvent) {
        // 按事件类型处理
        match event {
            // 收到一帧
            SocketEvent::Frame(value) => {
                // 记录入站时间；暂停握手期间丢弃内容
                if let Some(socket) = self.socket.as_mut() {
                    // 记录最近一次收到数据的时刻
                    socket.last_inbound = Instant::now();
                    // 已暂停
                    if socket.pausing {
                        // 忽略
                        return;
                    }
                }
                // 处理失败则断开并重试
                if let Err(e) = self.received(value) {
                    // 处理出错
                    self.failed(e);
                    // 返回
                    return;
                }
                // 处理成功后继续读取下一帧
                if let Some(socket) = &self.socket {
                    // 推进套接字状态
                    socket.advance();
                }
            }
            // 发送队列已清空
            SocketEvent::Drained => self.drained(),
            // 连接关闭
            SocketEvent::Closed(reason) => {
                // 没有套接字则返回
                let Some(socket) = &self.socket else { return };
                // 配对确认已发完：配对完成
                if socket.paired_close && socket.drained() {
                    // 配对完成
                    self.pairing_completed();
                // 暂停中：丢弃连接
                } else if socket.pausing {
                    // 丢弃套接字
                    self.drop_socket();
                // 否则视为失败
                } else {
                    // 记录失败原因
                    self.failed(reason);
                }
            }
        }
    }

    // 发送队列清空后的收尾
    pub(super) fn drained(&mut self) {
        // 没有套接字则返回
        let Some(socket) = &self.socket else { return };
        // 配对后关闭
        if socket.paired_close {
            // 配对完成
            self.pairing_completed();
        // 已暂停
        } else if socket.pausing {
            // 丢弃套接字
            self.drop_socket();
        }
    }

    /// 保存 ACK 写完即结束配对连接，稍后以新 TCP 和已存秘密认证。
    // 保存确认写完即结束配对连接，稍后用新的 TCP 与已保存的秘密认证
    pub(super) fn pairing_completed(&mut self) {
        // 丢弃套接字
        self.drop_socket();
        // 清除配对状态
        self.clear_pairing();
        // 500 毫秒后重连
        self.retry_at = Some(Instant::now() + Duration::from_millis(500));
        // 发布视图
        self.publish();
    }
}
