//! 用户命令只改变同一个连接 owner 的状态，不自行创建网络线程。
use super::{
    Command, Event, Handle, now_ms,
    owner::{Lifecycle, Owner},
    storage,
};
use loki_metris_pairing::valid_device_name;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

const PAIRING_WINDOW: Duration = Duration::from_secs(60);

/// revision 随取消意图提交，OS 同步存储尚未返回时也能撤销保存 ACK。
pub(super) struct QueuedCommand {
    pub(super) command: Command,
    pub(super) revision: u64,
}
/// 记录成功入队的选择意图。存储阻塞时，公开 view 可能还停留在上一个桌面。
#[derive(Default)]
pub(super) struct CommandIntent {
    pub(super) selected: Option<String>,
    pub(super) stopped: bool,
}
impl Handle {
    pub(super) fn enqueue(&self, command: Command) -> Result<(), String> {
        let mut intent = self.intent.lock().unwrap_or_else(|p| p.into_inner());
        if intent.stopped {
            return Err("连接已关闭".into());
        }
        let interrupts = match &command {
            Command::Select { .. } | Command::Active { value: false } | Command::Stop => true,
            Command::Forget { client_id } => intent.selected.as_ref() == Some(client_id),
            _ => false,
        };
        let revision = if interrupts {
            self.revision
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| "配对取消状态已用尽，请重新启动应用。")?
                + 1
        } else {
            self.revision.load(Ordering::SeqCst)
        };
        let selected = match &command {
            Command::Select { client_id } => Some(Some(client_id.clone())),
            Command::Forget { client_id } if intent.selected.as_ref() == Some(client_id) => {
                Some(None)
            }
            _ => None,
        };
        let stopped = matches!(command, Command::Stop);
        // 队列满也不撤回取消代数：已经请求撤销的保存资格必须保持失效。
        self.tx
            .try_send(Event::Command(QueuedCommand { command, revision }))
            .map_err(|_| "操作过快，请稍后重试。".to_owned())?;
        if let Some(selected) = selected {
            intent.selected = selected;
        }
        intent.stopped = stopped;
        Ok(())
    }
}
impl Owner {
    pub(super) fn apply(&mut self, queued: QueuedCommand) -> Result<(), String> {
        self.command_at(queued.command, queued.revision)
    }
    #[cfg(test)]
    pub(super) fn command(&mut self, c: Command) -> Result<(), String> {
        self.command_at(c, self.revision.load(Ordering::SeqCst))
    }
    fn command_at(&mut self, c: Command, revision: u64) -> Result<(), String> {
        match c {
            Command::Discover => {
                self.view["error"] = Value::Null;
                self.rediscover();
            }
            Command::Select { client_id } => self.select(client_id, revision)?,
            Command::Forget { client_id } => {
                let mut records = self.records.clone();
                records.remove(&client_id);
                self.save_records(&records)?;
                self.records = records;
                if self.selected.as_deref() == Some(&client_id)
                    || self.view["cacheSourceId"].as_str() == Some(&client_id)
                {
                    self.cancel();
                    self.selected = None;
                    self.view["selectedId"] = Value::Null;
                    storage::erase_snapshot(&self.root);
                    self.clear_display();
                }
                self.view["status"] = json!("已忘记，可重新配对");
            }
            Command::Rename { name } => {
                if !valid_device_name(&name) {
                    return Err("设备名称无效".into());
                }
                storage::save_name(&self.root, &self.id, &name)?;
                self.name = name;
                self.view["deviceName"] = json!(self.name);
            }
            Command::Active { value } => self.set_lifecycle(Lifecycle {
                visible: value,
                permitted: true,
            }),
            Command::Refresh => {
                if self.capabilities.contains("snapshotQueryV1") && self.view["connected"] == true {
                    self.send_secure(json!({"type":"snapshotRequest"}))?;
                } else if self.wire.is_none() {
                    self.retries = 0;
                    self.update_discovery();
                    if self.selected.is_some() {
                        self.retry_at = Some(Instant::now());
                    }
                }
            }
            Command::Host(command) => self.host(command)?,
            Command::Stop => {}
        }
        self.publish();
        Ok(())
    }

    /// 用户选择才开启首次密码窗口；已信任桌面只用保存的秘密认证。
    fn select(&mut self, client_id: String, revision: u64) -> Result<(), String> {
        if !self.endpoints.values().any(|e| e.id == client_id) {
            return Err("桌面当前不可达，请重新发现。".into());
        }
        let switched = self.selected.as_deref() != Some(&client_id);
        self.cancel();
        self.selected = Some(client_id.clone());
        self.view["selectedId"] = json!(client_id);
        self.view["error"] = Value::Null;
        self.retries = 0;
        if switched {
            storage::erase_snapshot(&self.root);
            self.clear_display();
        }
        // 中间选择仍须按 FIFO 生效，后续 Forget 才能忘掉正确对象；
        // 其保存资格已撤销时，只更新选择，不开启配对窗口或连接。
        if revision != self.revision.load(Ordering::SeqCst) {
            return Ok(());
        }
        if !self.records.contains_key(&client_id) {
            // 配对窗口只在前台开启；后台到达的选择只保留选中项。
            if !self.lifecycle.visible {
                return Ok(());
            }
            let now = Instant::now();
            let code = format!("{:04}", uuid::Uuid::new_v4().as_u128() % 10_000);
            self.protocol
                .start_pairing(&client_id, &self.name, code.clone(), revision, now)?;
            self.pairing_offer = Some(
                json!({"kind":"pairing","value":{"version":1,"clientId":client_id,"deviceId":self.id,"name":self.name}}),
            );
            self.pairing_deadline = Some(now + PAIRING_WINDOW);
            self.view["pairingCode"] = json!(code);
            self.view["pairingDeadlineMs"] = json!(now_ms() + PAIRING_WINDOW.as_millis() as u64);
        }
        if let Err(e) = self.connect() {
            self.failed(e);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "commands_test.rs"]
mod commands_test;
