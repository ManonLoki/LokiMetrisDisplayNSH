//! 当前认证iOS父连接将实际Watch回执发送为既有companionStatus；不自行证明在线。

use super::{Event, framed_socket::Phase, owner::Owner};
use crate::native::companion::presence::{Command, ProbeRequest, WireReport};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// 回调资格独立于socket数字代数，防止owner重建后旧事件数字碰撞。
pub(super) struct Relay {
    pub(super) scope: String,
    pub(super) ping: Option<(u64, Instant)>,
}
impl Default for Relay {
    /// 每个owner先创建唯一资格，首次ping之前无可用nonce。
    fn default() -> Self {
        Self {
            scope: uuid::Uuid::new_v4().to_string(),
            ping: None,
        }
    }
}
impl Relay {
    /// 关闭/暂停/换目标立即撤销回调；下一连接不能继承旧nonce。
    pub(super) fn reset(&mut self) {
        crate::native::companion::presence_command(Command::Cancel(self.scope.clone()));
        self.scope = uuid::Uuid::new_v4().to_string();
        self.ping = None;
    }
}
impl Owner {
    /// 必须在pong已经成功入队后调用；nonce=0合法，能力取当前完整认证快照。
    pub(super) fn watch_ping(&mut self, nonce: u64) {
        if !self.watch_companion
            || self.platform != "ios"
            || !self.active()
            || !self.capabilities.contains("watchCompanionV1")
            || !self.view["connected"].as_bool().unwrap_or(false)
        {
            return;
        }
        let Some(source_id) = self.selected.clone() else {
            return;
        };
        self.watch.ping = Some((nonce, Instant::now()));
        let generation = self.generation;
        let scope = self.watch.scope.clone();
        let captured_scope = scope.clone();
        let events = self.events.clone();
        crate::native::companion::presence_command(Command::Probe(ProbeRequest {
            root: self.root.join("companion-v1"),
            source_id,
            scope,
            sink: Arc::new(move |report| {
                let _ = events.try_send(Event::Companion {
                    generation,
                    scope: captured_scope.clone(),
                    nonce,
                    report,
                });
            }),
        }));
    }

    /// 迟到、非iOS、后台、已关闭与旧nonce的事件仅丢弃，绝不污染新连接或关闭正常父连接。
    pub(super) fn watch_report(
        &mut self,
        generation: u64,
        scope: &str,
        nonce: u64,
        report: WireReport,
    ) {
        let eligible = generation == self.generation
            && scope == self.watch.scope
            && self.watch_companion
            && self.platform == "ios"
            && self.active()
            && self.capabilities.contains("watchCompanionV1")
            && self
                .socket
                .as_ref()
                .is_some_and(|socket| socket.phase == Phase::Streaming && !socket.pausing)
            && self.watch.ping.is_some_and(|(current, at)| {
                current == nonce && at.elapsed() < Duration::from_secs(45)
            })
            && report.info.device_id != self.id
            && self.selected.as_ref() != Some(&report.info.device_id);
        if !eligible {
            return;
        }
        // 序列化与加密沿原owner发送路径；失败按现有连接失败处理，不造明文旁路。
        if let Err(error) = self.send_secure(
            serde_json::json!({"type":"companionStatus","nonce":nonce,"report":report}),
        ) {
            self.failed(error);
        }
    }
}
