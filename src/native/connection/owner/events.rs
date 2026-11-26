use super::*;
use crate::native::connection::{
    discovery::Found,
    framed_socket::{SocketAction, SocketTimeout},
};

impl Owner {
    // 最近的期限：配对窗口、重连、套接字、资源传输与发现退避中最早的一个
    pub(super) fn next_deadline(&self) -> Option<Instant> {
        [
            self.pairing_deadline,
            self.retry_at,
            self.socket
                .as_ref()
                .and_then(FramedSocket::next_deadline)
                .map(|(deadline, _)| deadline),
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
        if let Some((deadline, reason)) = self.socket.as_ref().and_then(FramedSocket::next_deadline)
            && now >= deadline
        {
            match reason {
                SocketTimeout::Pause => self.drop_socket(),
                SocketTimeout::Response => self.failed("连接超时，正在重试…".into()),
                SocketTimeout::Idle => self.failed("桌面暂不可达，恢复网络后重新连接。".into()),
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
        let Some(action) = self
            .socket
            .as_mut()
            .and_then(|socket| socket.handle_event(event))
        else {
            return;
        };
        match action {
            SocketAction::Receive(value) => {
                if let Err(e) = self.received(value) {
                    self.failed(e);
                    return;
                }
                // 处理成功后继续读取下一帧
                if let Some(socket) = &self.socket {
                    socket.advance();
                }
            }
            SocketAction::PairingComplete => self.pairing_completed(),
            SocketAction::Disconnect => self.drop_socket(),
            SocketAction::Failed(reason) => self.failed(reason),
        }
    }

    /// 保存 ACK 写完即结束配对连接，稍后以新 TCP 和已存秘密认证。
    // 保存确认写完即结束配对连接，稍后用新的 TCP 与已保存的秘密认证
    fn pairing_completed(&mut self) {
        self.drop_socket();
        self.clear_pairing();
        // 500 毫秒后重连
        self.retry_at = Some(Instant::now() + Duration::from_millis(500));
        self.publish();
    }

    pub(super) fn found(&mut self, found: Found) {
        match found {
            Found::Resolved {
                key,
                id,
                name,
                address,
            } => {
                if id == self.id
                    || (self.endpoints.len() >= 64 && !self.endpoints.contains_key(&key))
                {
                    return;
                }
                self.discovery.record_resolution();
                let moved = self
                    .endpoints
                    .get(&key)
                    .is_some_and(|endpoint| endpoint.address != address);
                self.endpoints.insert(
                    key,
                    Endpoint {
                        id: id.clone(),
                        name,
                        address,
                    },
                );
                let selected = self.selected.as_deref() == Some(id.as_str());
                if moved
                    && selected
                    && self
                        .socket
                        .as_ref()
                        .is_some_and(|socket| socket.peer() != address)
                {
                    self.failed("桌面地址已变化，正在恢复连接…".into());
                } else if self.selected.is_none() && self.records.contains_key(&id) {
                    self.selected = Some(id.clone());
                    self.view["selectedId"] = json!(id);
                    self.retry_at = Some(Instant::now());
                } else if self.socket.is_none() && selected && self.active() {
                    self.retries = 0;
                    self.retry_at = Some(Instant::now());
                }
            }
            Found::Removed(key) => {
                self.endpoints.remove(&key);
            }
            Found::Failed(message) => {
                self.view["error"] = json!(message);
                self.discovery.record_failure();
            }
        }
        self.publish();
    }
}
