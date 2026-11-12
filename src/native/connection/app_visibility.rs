//! 应用可见性和局域网权限驱动连接所有者。
use super::{
    framed_socket::Phase,
    owner::{Lifecycle, Owner},
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

// 暂停握手的等待上限：3 秒
const PAUSE_BUDGET: Duration = Duration::from_secs(3);

impl Owner {
    // 应用可见性或权限变化：暂停或恢复连接与发现
    pub(super) fn set_lifecycle(&mut self, next: Lifecycle) {
        let was_active = self.active();
        let was_visible = self.lifecycle.visible;
        // 采用新的生命周期状态
        self.lifecycle = next;
        // 离开前台时取消进行中的配对
        if was_visible && !next.visible && self.pairing_offer.is_some() {
            self.cancel();
            self.view["status"] = json!("配对已暂停，请回到前台重新选择桌面。");
        }
        let active = self.active();
        // 由活跃变为不活跃：暂停，并按原因给出状态
        if was_active && !active {
            self.pause();
            // 缺权限与进入后台提示不同
            self.view["status"] = json!(if next.visible && !next.permitted {
                "需要本地网络权限"
            // 否则分支
            } else {
                // 已暂停，回到前台后恢复
                "已暂停，回到前台后恢复"
            });
        // 由不活跃变为活跃：重置重试并必要时立即重连
        } else if !was_active && active {
            self.retries = 0;
            self.view["error"] = Value::Null;
            // 正在暂停握手的连接直接丢弃
            if self.socket.as_ref().is_some_and(|socket| socket.pausing) {
                self.drop_socket();
            }
            // 已信任的桌面立刻重试
            if self.socket.is_none() && self.selected_trusted() {
                self.retry_at = Some(Instant::now());
            }
        }
        self.update_discovery();
        self.publish();
    }

    // 暂停：清掉重试、配对与实时状态，并礼貌地告知桌面
    fn pause(&mut self) {
        #[cfg(any(target_os = "ios", test))]
        self.watch.reset();
        self.retry_at = None;
        self.clear_pairing();
        self.clear_live();
        // 正在推送快照且尚未暂停
        let streaming = self
            .socket
            .as_ref()
            // 处于传输阶段且未暂停
            .is_some_and(|socket| socket.phase == Phase::Streaming && !socket.pausing);
        // 发送 pause 后在 3 秒内等待桌面确认
        if streaming && self.send_secure(json!({"type":"pause"})).is_ok() {
            if let Some(socket) = self.socket.as_mut() {
                socket.pausing = true;
                // 设定暂停预算内的期限
                socket.deadline = Instant::now() + PAUSE_BUDGET;
            }
        // 否则直接断开
        } else {
            self.drop_socket();
        }
    }

    // 当前选中的桌面是否已有配对记录
    pub(super) fn selected_trusted(&self) -> bool {
        self.selected
            .as_ref()
            // 选中项在配对记录里
            .is_some_and(|id| self.records.contains_key(id))
    }

    // 按需启停发现：活跃且（可见或未在推送）才需要
    pub(super) fn update_discovery(&mut self) {
        let streaming = self
            .socket
            .as_ref()
            .is_some_and(|socket| socket.phase == Phase::Streaming);
        // 活动，且可见或未在传输时才需要发现
        let wanted = self.active() && (self.lifecycle.visible || !streaming);
        // 需要且尚未运行、也不在退避中才启动
        if wanted {
            if !self.discovery.running()
                && self.discovery.retry_at().is_none()
                && let Err(error) = self.discovery.start(self.events.clone())
            {
                // 启动失败则展示错误
                self.view["error"] = json!(error);
            }
        } else {
            self.discovery.stop();
        }
    }

    // 重新发现：清空端点后重启
    pub(super) fn rediscover(&mut self) {
        self.endpoints.clear();
        self.discovery.stop();
        self.update_discovery();
    }
}

#[cfg(test)]
#[path = "app_visibility_test.rs"]
mod app_visibility_test;
