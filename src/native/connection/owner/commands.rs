//! 在 owner 上按入队顺序应用用户命令。
use super::{Lifecycle, Owner};
use crate::native::connection::{Command, commands::QueuedCommand, now_ms, persistence};
use loki_metris_pairing::valid_device_name;
use serde_json::{Value, json};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

// 首次配对窗口：60 秒
const PAIRING_WINDOW: Duration = Duration::from_secs(60);

impl Owner {
    // 应用一条已入队的命令
    pub(super) fn apply(&mut self, queued: QueuedCommand) -> Result<(), String> {
        // 按入队时的修订号执行命令
        self.command_at(queued.command, queued.revision)
    }
    #[cfg(test)]
    pub(super) fn command(&mut self, c: Command) -> Result<(), String> {
        // 按当前修订号执行命令
        self.command_at(c, self.revision.load(Ordering::SeqCst))
    }
    // 执行命令；完成后发布新投影
    fn command_at(&mut self, c: Command, revision: u64) -> Result<(), String> {
        match c {
            // 重新发现：清除错误并重启发现
            Command::Discover => {
                self.view["error"] = Value::Null;
                self.rediscover();
            }
            Command::Select { client_id } => self.select(client_id, revision)?,
            // 忘记桌面：先撤销小组件授权，再提交记录
            Command::Forget { client_id } => {
                let mut records = self.records.clone();
                records.remove(&client_id);
                // 在提交一次成功的遗忘之前,先撤销扩展的授权。
                // 先撤销扩展授权，成功后才提交忘记
                self.forget_widget_source(&client_id)?;
                self.save_records(&records)?;
                self.records = records;
                // 忘记的是当前展示的桌面：取消连接、清空显示与本地快照
                if self.selected.as_deref() == Some(&client_id)
                    // 或者缓存来源是该客户端
                    || self.view["cacheSourceId"].as_str() == Some(&client_id)
                {
                    self.cancel();
                    self.selected = None;
                    // 清除视图里的选中项
                    self.view["selectedId"] = Value::Null;
                    persistence::erase_snapshot(&self.root);
                    self.clear_display();
                }
                // 状态：已忘记，可重新配对
                self.view["status"] = json!("已忘记，可重新配对");
            }
            // 重命名：校验后落盘并更新视图
            Command::Rename { name } => {
                if !valid_device_name(&name) {
                    return Err("设备名称无效".into());
                }
                persistence::save_name(&self.root, &self.id, &name)?;
                self.name = name;
                self.view["deviceName"] = json!(self.name);
            }
            Command::Active { value } => self.set_lifecycle(Lifecycle {
                visible: value,
                permitted: true,
            }),
            // 刷新：已连接且支持快照查询就请求快照，没有连接则重试发现
            Command::Refresh => {
                // 支持快照查询且已连接
                if self.capabilities.contains("snapshotQueryV1") && self.view["connected"] == true {
                    self.send_secure(json!({"type":"snapshotRequest"}))?;
                } else if self.socket.is_none() {
                    self.retries = 0;
                    self.update_discovery();
                    if self.selected.is_some() {
                        self.retry_at = Some(Instant::now());
                    }
                }
            }
            // 停止由 owner 主循环处理
            Command::Stop => {}
        }
        self.publish();
        Ok(())
    }

    /// 用户选择才开启首次密码窗口；已信任桌面只用保存的秘密认证。
    // 用户选择桌面才开启首次配对窗口；已信任的桌面只用保存的秘密认证
    fn select(&mut self, client_id: String, revision: u64) -> Result<(), String> {
        if !self.endpoints.values().any(|e| e.id == client_id) {
            return Err("桌面当前不可达，请重新发现。".into());
        }
        let switched = self.selected.as_deref() != Some(&client_id);
        // 切换时小组件共享存储须先撤销；失败则不能悄悄只切换应用
        if switched {
            // 共享存储撤销失败时,不得悄悄地只切换应用本身。
            self.select_widget_source(&client_id)?;
        }
        self.cancel();
        self.selected = Some(client_id.clone());
        // 更新视图里的选中项
        self.view["selectedId"] = json!(client_id);
        self.view["error"] = Value::Null;
        self.retries = 0;
        // 换桌面就清掉旧快照与显示
        if switched {
            persistence::erase_snapshot(&self.root);
            self.clear_display();
        }
        // 中间选择仍须按 FIFO 生效，后续 Forget 才能忘掉正确对象；
        // 其保存资格已撤销时，只更新选择，不开启配对窗口或连接。
        // 中间选择仍按 FIFO 生效；其保存资格已撤销时只更新选择，不开启配对窗口或连接
        if revision != self.revision.load(Ordering::SeqCst) {
            return Ok(());
        }
        // 未配对的桌面需要配对码
        if !self.records.contains_key(&client_id) {
            // 配对窗口只在前台开启；后台到达的选择只保留选中项。
            // 配对窗口只在前台开启
            if !self.lifecycle.visible {
                return Ok(());
            }
            let now = Instant::now();
            // 生成四位随机配对码
            let code = format!("{:04}", uuid::Uuid::new_v4().as_u128() % 10_000);
            self.protocol
                .start_pairing(&client_id, &self.name, code.clone(), revision, now)?;
            self.pairing_offer = Some(
                json!({"kind":"pairing","value":{"version":1,"clientId":client_id,"deviceId":self.id,"name":self.name}}),
            );
            self.pairing_deadline = Some(now + PAIRING_WINDOW);
            // 把配对码与截止时间写入公开视图
            self.view["pairingCode"] = json!(code);
            // 配对期限（毫秒时间戳）
            self.view["pairingDeadlineMs"] = json!(now_ms() + PAIRING_WINDOW.as_millis() as u64);
        }
        // 发起连接，失败则标记为失败状态
        if let Err(e) = self.connect() {
            self.failed(e);
        }
        Ok(())
    }
}
