//! NSH 应用可见性与网络变化驱动唯一的连接所有者。
// 引入宿主命令、套接字阶段与 owner
use super::{
    // 宿主命令
    HostCommand,
    // 传输阶段
    framed_socket::Phase,
    // 生命周期与所有者
    owner::{Lifecycle, Owner},
};
// 引入 JSON
use serde_json::{Value, json};
// 引入时间
use std::time::{Duration, Instant};

// 暂停握手的等待上限：3 秒
const PAUSE_BUDGET: Duration = Duration::from_secs(3);

// 所有者的方法
impl Owner {
    // 应用可见性或权限变化：暂停或恢复连接与发现
    pub(super) fn set_lifecycle(&mut self, next: Lifecycle) {
        // 记录变化前的状态
        let was_active = self.active();
        // 此前是否可见
        let was_visible = self.lifecycle.visible;
        // 采用新的生命周期状态
        self.lifecycle = next;
        // 离开前台时取消进行中的配对
        if was_visible && !next.visible && self.pairing_offer.is_some() {
            // 取消配对
            self.cancel();
            // 状态：配对已暂停
            self.view["status"] = json!("配对已暂停，请回到前台重新选择桌面。");
        }
        // 当前是否活动
        let active = self.active();
        // 由活跃变为不活跃：暂停，并按原因给出状态
        if was_active && !active {
            // 暂停连接
            self.pause();
            // 缺权限与进入后台提示不同
            self.view["status"] = json!(if next.visible && !next.permitted {
                // 需要本地网络权限
                "需要本地网络权限"
            // 否则分支
            } else {
                // 已暂停，回到前台后恢复
                "已暂停，回到前台后恢复"
            });
        // 由不活跃变为活跃：重置重试并必要时立即重连
        } else if !was_active && active {
            // 重试计数归零
            self.retries = 0;
            // 清除错误
            self.view["error"] = Value::Null;
            // 正在暂停握手的连接直接丢弃
            if self.socket.as_ref().is_some_and(|socket| socket.pausing) {
                // 丢弃套接字
                self.drop_socket();
            }
            // 已信任的桌面立刻重试
            if self.socket.is_none() && self.selected_trusted() {
                // 立即安排重试
                self.retry_at = Some(Instant::now());
            }
        }
        // 更新发现状态
        self.update_discovery();
        // 发布视图
        self.publish();
    }

    // 暂停：清掉重试、配对与实时状态，并礼貌地告知桌面
    fn pause(&mut self) {
        // 清除重试时刻
        self.retry_at = None;
        // 清除配对状态
        self.clear_pairing();
        // 清除实时数据
        self.clear_live();
        // 正在推送快照且尚未暂停
        let streaming = self
            // 套接字
            .socket
            // 取引用
            .as_ref()
            // 处于传输阶段且未暂停
            .is_some_and(|socket| socket.phase == Phase::Streaming && !socket.pausing);
        // 发送 pause 后在 3 秒内等待桌面确认
        if streaming && self.send_secure(json!({"type":"pause"})).is_ok() {
            // 有套接字时
            if let Some(socket) = self.socket.as_mut() {
                // 标记暂停中
                socket.pausing = true;
                // 设定暂停预算内的期限
                socket.deadline = Instant::now() + PAUSE_BUDGET;
            }
        // 否则直接断开
        } else {
            // 丢弃套接字
            self.drop_socket();
        }
    }

    // 当前选中的桌面是否已有配对记录
    pub(super) fn selected_trusted(&self) -> bool {
        // 当前选中项
        self.selected
            // 取引用
            .as_ref()
            // 选中项在配对记录里
            .is_some_and(|id| self.records.contains_key(id))
    }

    // 处理宿主命令
    pub(super) fn host(&mut self, command: HostCommand) -> Result<(), String> {
        // 按宿主命令处理
        match command {
            // 前后台与权限
            HostCommand::Lifecycle { visible, permitted } => {
                // 更新生命周期
                self.set_lifecycle(Lifecycle { visible, permitted })
            }
            // 网络变化：清空端点、停止发现
            HostCommand::Network => {
                // 清空端点
                self.endpoints.clear();
                // 停止发现
                self.discovery.stop();
                // 尚未进入推送阶段的连接视为失败，重新发现
                if self
                    // 套接字
                    .socket
                    // 取引用
                    .as_ref()
                    // 不处于传输阶段
                    .is_some_and(|socket| socket.phase != Phase::Streaming)
                {
                    // 网络已变化，重新发现
                    self.failed("网络已变化，正在重新发现桌面端…".into());
                }
                // 更新发现状态
                self.update_discovery();
                // 发布视图
                self.publish();
            }
        }
        // 完成
        Ok(())
    }

    // 按需启停发现：活跃且（可见或未在推送）才需要
    pub(super) fn update_discovery(&mut self) {
        // 是否处于传输阶段
        let streaming = self
            // 套接字
            .socket
            // 取引用
            .as_ref()
            // 处于传输阶段
            .is_some_and(|socket| socket.phase == Phase::Streaming);
        // 活动，且可见或未在传输时才需要发现
        let wanted = self.active() && (self.lifecycle.visible || !streaming);
        // 需要且尚未运行、也不在退避中才启动
        if wanted {
            // 发现尚未运行
            if !self.discovery.running()
                // 并且没有重试时刻
                && self.discovery.retry_at().is_none()
                // 启动发现出错
                && let Err(error) = self.discovery.start(self.events.clone())
            {
                // 启动失败则展示错误
                self.view["error"] = json!(error);
            }
        // 不需要则停止
        } else {
            // 停止发现
            self.discovery.stop();
        }
    }

    // 重新发现：清空端点后重启
    pub(super) fn rediscover(&mut self) {
        // 清空端点
        self.endpoints.clear();
        // 停止发现
        self.discovery.stop();
        // 更新发现状态
        self.update_discovery();
    }
}

// 测试模块
#[cfg(test)]
#[path = "app_visibility_test.rs"]
mod app_visibility_test;
