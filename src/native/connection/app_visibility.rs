//! NSH application visibility and network changes drive the single connection owner.
use super::{
    HostCommand,
    framed_socket::Phase,
    owner::{Lifecycle, Owner},
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const PAUSE_BUDGET: Duration = Duration::from_secs(3);

impl Owner {
    pub(super) fn set_lifecycle(&mut self, next: Lifecycle) {
        let was_active = self.active();
        let was_visible = self.lifecycle.visible;
        self.lifecycle = next;
        if was_visible && !next.visible && self.pairing_offer.is_some() {
            self.cancel();
            self.view["status"] = json!("配对已暂停，请回到前台重新选择桌面。");
        }
        let active = self.active();
        if was_active && !active {
            self.pause();
            self.view["status"] = json!(if next.visible && !next.permitted {
                "需要本地网络权限"
            } else {
                "已暂停，回到前台后恢复"
            });
        } else if !was_active && active {
            self.retries = 0;
            self.view["error"] = Value::Null;
            if self.socket.as_ref().is_some_and(|socket| socket.pausing) {
                self.drop_socket();
            }
            if self.socket.is_none() && self.selected_trusted() {
                self.retry_at = Some(Instant::now());
            }
        }
        self.update_discovery();
        self.publish();
    }

    fn pause(&mut self) {
        self.retry_at = None;
        self.clear_pairing();
        self.clear_live();
        let streaming = self
            .socket
            .as_ref()
            .is_some_and(|socket| socket.phase == Phase::Streaming && !socket.pausing);
        if streaming && self.send_secure(json!({"type":"pause"})).is_ok() {
            if let Some(socket) = self.socket.as_mut() {
                socket.pausing = true;
                socket.deadline = Instant::now() + PAUSE_BUDGET;
            }
        } else {
            self.drop_socket();
        }
    }

    pub(super) fn selected_trusted(&self) -> bool {
        self.selected
            .as_ref()
            .is_some_and(|id| self.records.contains_key(id))
    }

    pub(super) fn host(&mut self, command: HostCommand) -> Result<(), String> {
        match command {
            HostCommand::Lifecycle { visible, permitted } => {
                self.set_lifecycle(Lifecycle { visible, permitted })
            }
            HostCommand::Network => {
                self.endpoints.clear();
                self.discovery.stop();
                if self
                    .socket
                    .as_ref()
                    .is_some_and(|socket| socket.phase != Phase::Streaming)
                {
                    self.failed("网络已变化，正在重新发现桌面端…".into());
                }
                self.update_discovery();
                self.publish();
            }
        }
        Ok(())
    }

    pub(super) fn update_discovery(&mut self) {
        let streaming = self
            .socket
            .as_ref()
            .is_some_and(|socket| socket.phase == Phase::Streaming);
        let wanted = self.active() && (self.lifecycle.visible || !streaming);
        if wanted {
            if !self.discovery.running()
                && self.discovery.retry_at().is_none()
                && let Err(error) = self.discovery.start(self.events.clone())
            {
                self.view["error"] = json!(error);
            }
        } else {
            self.discovery.stop();
        }
    }

    pub(super) fn rediscover(&mut self) {
        self.endpoints.clear();
        self.discovery.stop();
        self.update_discovery();
    }
}

#[cfg(test)]
#[path = "app_visibility_test.rs"]
mod app_visibility_test;
