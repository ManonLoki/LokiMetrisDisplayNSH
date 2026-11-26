//! 连接、退避、可见性和安全帧发送都在 owner 线程推进。
use super::{Lifecycle, Owner};
use crate::native::connection::{
    err,
    framed_socket::{FramedSocket, Phase},
    persistence::decode_secret,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

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
            if self
                .socket
                .as_ref()
                .is_some_and(|socket| socket.phase() == Phase::Pausing)
            {
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

    // 当前选中的桌面是否已有配对记录
    fn selected_trusted(&self) -> bool {
        self.selected
            .as_ref()
            // 选中项在配对记录里
            .is_some_and(|id| self.records.contains_key(id))
    }

    // 仅在可见且允许联网时进行发现
    pub(super) fn update_discovery(&mut self) {
        if self.active() {
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

    // 取得当前套接字，没有则报“连接已关闭”
    pub(super) fn socket_mut(&mut self) -> Result<&mut FramedSocket, String> {
        // 取出套接字，已关闭则报错
        self.socket.as_mut().ok_or_else(|| "连接已关闭".into())
    }

    // 连接选中的桌面：配对用邀请，已信任用保存的秘密恢复
    pub(super) fn connect(&mut self) -> Result<(), String> {
        if !self.active() {
            return Ok(());
        }
        let id = self.selected.clone().ok_or("尚未选择桌面")?;
        // 该桌面当前的端点；必须唯一
        let values: Vec<_> = self.endpoints.values().filter(|e| e.id == id).collect();
        if values.len() != 1 {
            // 冷启动短暂失败/无结果之后,浏览器可能仍在运行。
            // 恢复重试时必须同时重试发现,而不仅仅是 TCP。
            // 冷启动时发现可能尚未结束；恢复重试时也要重试发现，而不只是 TCP
            if values.is_empty() && self.discovery.retry_at().is_none() {
                self.rediscover();
            }
            // 桌面不可达或身份重复
            return Err("桌面不可达或存在重复身份，正在重新发现…".into());
        }
        // 取第一个候选的地址
        let address = values[0].address;
        let pairing_deadline = self
            .pairing_offer
            .as_ref()
            .map(|_| self.pairing_deadline.ok_or("配对已过期"))
            .transpose()?;
        // 开场消息：配对邀请，或用保存的秘密生成的恢复请求
        let intro = if let Some(offer) = &self.pairing_offer {
            offer.clone()
        // 否则分支
        } else {
            // 查找已保存的配对记录
            let record = self
                .records
                .get(&id)
                // 找不到则要求重新选择桌面
                .ok_or("配对记录不可用，请重新选择桌面")?;
            let secret = decode_secret(&record.secret).ok_or("安全存储记录无效")?;
            let hello = self.protocol.resume(&id, &self.id, &secret)?;
            json!({"kind":"resume","value":hello})
        };
        self.drop_socket();
        self.generation += 1;
        let mut socket = FramedSocket::connect(
            address,
            pairing_deadline,
            self.generation,
            self.events.clone(),
        );
        socket.send(&intro)?;
        self.socket = Some(socket);
        self.form_factor_reported = false;
        // 视图显示为未连接并给出提示
        self.view["connected"] = json!(false);
        self.view["live"] = json!(false);
        // 按是否配对设置状态
        self.view["status"] = json!(if pairing_deadline.is_some() {
            "等待在桌面输入手机配对码"
        // 否则分支
        } else {
            "正在认证桌面…"
        });
        self.publish();
        Ok(())
    }

    // 加密并发送一条消息
    pub(super) fn send_secure(&mut self, v: Value) -> Result<(), String> {
        let result = (|| {
            let packet = self.protocol.seal(&serde_json::to_vec(&v).map_err(err)?)?;
            self.socket_mut()?
                .send(&serde_json::to_value(packet).map_err(err)?)
        })();
        // seal 已消耗序号；排队失败时必须关闭整条通道，不能带着序号缺口继续发送。
        // seal 已消耗序号；排队失败必须关闭整条通道，不能带着序号缺口继续发送
        if let Err(error) = &result {
            // 发送失败，记录失败
            self.failed(error.clone());
        }
        result
    }

    /// 关闭当前 TCP，保留选择与信任；同一配对窗口内允许按退避重试。
    // 关闭当前 TCP，保留选择与信任；按退避重试
    pub(super) fn failed(&mut self, message: String) {
        let had_transport = self.socket.is_some();
        self.drop_socket();
        self.protocol.disconnect();
        self.clear_live();
        // 状态：连接中断，正在重试
        self.view["status"] = json!("连接中断，正在重试…");
        self.view["error"] = json!(message);
        // 有选择且活跃时安排重试
        if self.selected.is_some() && self.active() && (had_transport || self.retry_at.is_none()) {
            self.retries = self.retries.saturating_add(1);
            // 退避时间：500ms 起每次翻倍至 8 秒，超过 8 次后固定 30 秒
            self.retry_at = Some(
                Instant::now()
                    + if self.retries <= 8 {
                        Duration::from_millis(
                            // 500 毫秒起指数增长
                            500u64.saturating_mul(1 << (self.retries.saturating_sub(1)).min(4)),
                        )
                    // 否则分支
                    } else {
                        Duration::from_secs(30)
                    },
            );
        }
        self.update_discovery();
        self.publish();
    }

    /// 撤销连接、配对窗口与重试。
    // 撤销连接、配对窗口与重试
    pub(super) fn cancel(&mut self) {
        self.drop_socket();
        self.retry_at = None;
        self.clear_pairing();
        self.protocol.cancel();
    }

    // 清除配对邀请、截止时间与视图中的配对码
    pub(super) fn clear_pairing(&mut self) {
        self.pairing_offer = None;
        self.pairing_deadline = None;
        self.view["pairingCode"] = Value::Null;
        self.view["pairingDeadlineMs"] = Value::Null;
    }

    /// 断线撤下实时状态；不保留离线快照的宿主同时清空展示。
    // 断线撤下实时状态；不保留离线快照的宿主同时清空展示
    pub(super) fn clear_live(&mut self) {
        self.view["connected"] = json!(false);
        self.view["live"] = json!(false);
        self.pause_widget_lease();
        if !self.keep_snapshot {
            self.clear_display();
        }
    }
}
