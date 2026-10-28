//! 用户命令只改变同一个连接 owner 的状态，不自行创建网络线程。
// 引入命令、事件、句柄与 owner
use super::{
    // 命令、事件、句柄与当前时间
    Command,
    // 事件
    Event,
    // 句柄
    Handle,
    // 当前时间
    now_ms,
    // 所有者与生命周期
    owner::{Lifecycle, Owner},
    // 持久化
    persistence,
};
// 引入名称校验
use loki_metris_pairing::valid_device_name;
// 引入 JSON
use serde_json::{Value, json};
// 引入原子序
use std::sync::atomic::Ordering;
// 引入时间
use std::time::{Duration, Instant};

// 首次配对窗口：60 秒
const PAIRING_WINDOW: Duration = Duration::from_secs(60);

/// revision 随取消意图提交，OS 同步存储尚未返回时也能撤销保存 ACK。
// 已入队的命令：随取消代数提交，OS 同步存储尚未返回时也能撤销保存确认
pub(super) struct QueuedCommand {
    // 命令
    pub(super) command: Command,
    // 修订号
    pub(super) revision: u64,
}
/// 记录成功入队的选择意图。存储阻塞时，公开 view 可能还停留在上一个桌面。
#[derive(Default)]
// 成功入队的选择意图；存储阻塞时公开 view 可能还停留在上一个桌面
pub(super) struct CommandIntent {
    // 当前意图选中的桌面
    pub(super) selected: Option<String>,
    // 是否已停止
    pub(super) stopped: bool,
}
// 句柄的方法
impl Handle {
    // 入队命令；停止后拒绝
    pub(super) fn enqueue(&self, command: Command) -> Result<(), String> {
        // 取意图状态
        let mut intent = self.intent.lock().unwrap_or_else(|p| p.into_inner());
        // 已停止
        if intent.stopped {
            // 报连接已关闭
            return Err("连接已关闭".into());
        }
        // 选择、置为不可见、停止，以及忘记当前选中的桌面，都会中断进行中的配对
        let interrupts = match &command {
            // 选择、停用或停止命令会使旧修订失效
            Command::Select { .. } | Command::Active { value: false } | Command::Stop => true,
            // 忘记命令且正是当前选中项
            Command::Forget { client_id } => intent.selected.as_ref() == Some(client_id),
            // 其余情况为否
            _ => false,
        };
        // 中断类命令递增取消代数；溢出则报错
        let revision = if interrupts {
            // 修订号
            self.revision
                // 原子地更新修订号
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                    // 加一
                    value.checked_add(1)
                })
                // 耗尽则报错
                .map_err(|_| "配对取消状态已用尽，请重新启动应用。")?
                // 加一
                + 1
        // 否则分支
        } else {
            // 读取当前修订号
            self.revision.load(Ordering::SeqCst)
        };
        // 计算入队成功后的新选择意图
        let selected = match &command {
            // 选择命令记录选中的客户端
            Command::Select { client_id } => Some(Some(client_id.clone())),
            // 忘记命令且正是当前选中项
            Command::Forget { client_id } if intent.selected.as_ref() == Some(client_id) => {
                // 其他命令清除选中项
                Some(None)
            }
            // 其余不改变
            _ => None,
        };
        // 是否为停止命令
        let stopped = matches!(command, Command::Stop);
        // 队列满也不撤回取消代数：已经请求撤销的保存资格必须保持失效。
        // 队列满也不撤回取消代数：已请求撤销的保存资格必须保持失效
        self.tx
            // 非阻塞发送命令事件
            .try_send(Event::Command(QueuedCommand { command, revision }))
            // 队列满则提示操作过快
            .map_err(|_| "操作过快，请稍后重试。".to_owned())?;
        // 入队成功后才更新意图
        if let Some(selected) = selected {
            // 更新选中项
            intent.selected = selected;
        }
        // 更新停止标记
        intent.stopped = stopped;
        // 完成
        Ok(())
    }
}
// 所有者的方法
impl Owner {
    // 应用一条已入队的命令
    pub(super) fn apply(&mut self, queued: QueuedCommand) -> Result<(), String> {
        // 按入队时的修订号执行命令
        self.command_at(queued.command, queued.revision)
    }
    // 测试直接调用
    #[cfg(test)]
    pub(super) fn command(&mut self, c: Command) -> Result<(), String> {
        // 按当前修订号执行命令
        self.command_at(c, self.revision.load(Ordering::SeqCst))
    }
    // 执行命令；完成后发布新投影
    fn command_at(&mut self, c: Command, revision: u64) -> Result<(), String> {
        // 按命令种类处理
        match c {
            // 重新发现：清除错误并重启发现
            Command::Discover => {
                // 清除错误
                self.view["error"] = Value::Null;
                // 重新发现
                self.rediscover();
            }
            // 选择桌面
            Command::Select { client_id } => self.select(client_id, revision)?,
            // 忘记桌面：先撤销小组件授权，再提交记录
            Command::Forget { client_id } => {
                // 复制配对记录
                let mut records = self.records.clone();
                // 移除指定客户端
                records.remove(&client_id);
                // 在提交一次成功的遗忘之前,先撤销扩展的授权。
                // 先撤销扩展授权，成功后才提交忘记
                self.forget_widget_source(&client_id)?;
                // 写入凭据库
                self.save_records(&records)?;
                // 替换记录表
                self.records = records;
                // 忘记的是当前展示的桌面：取消连接、清空显示与本地快照
                if self.selected.as_deref() == Some(&client_id)
                    // 或者缓存来源是该客户端
                    || self.view["cacheSourceId"].as_str() == Some(&client_id)
                {
                    // 取消配对
                    self.cancel();
                    // 清除选中项
                    self.selected = None;
                    // 清除视图里的选中项
                    self.view["selectedId"] = Value::Null;
                    // 删除快照缓存
                    persistence::erase_snapshot(&self.root);
                    // 清除显示
                    self.clear_display();
                }
                // 状态：已忘记，可重新配对
                self.view["status"] = json!("已忘记，可重新配对");
            }
            // 重命名：校验后落盘并更新视图
            Command::Rename { name } => {
                // 设备名无效
                if !valid_device_name(&name) {
                    // 报设备名称无效
                    return Err("设备名称无效".into());
                }
                // 保存设备名
                persistence::save_name(&self.root, &self.id, &name)?;
                // 更新内存里的名称
                self.name = name;
                // 更新视图里的名称
                self.view["deviceName"] = json!(self.name);
            }
            // 桌面端可见性变化
            Command::Active { value } => self.set_lifecycle(Lifecycle {
                // 可见性取新值
                visible: value,
                // 允许
                permitted: true,
            }),
            // 刷新：已连接且支持快照查询就请求快照，没有连接则重试发现
            Command::Refresh => {
                // 支持快照查询且已连接
                if self.capabilities.contains("snapshotQueryV1") && self.view["connected"] == true {
                    // 请求最新快照
                    self.send_secure(json!({"type":"snapshotRequest"}))?;
                // 没有套接字
                } else if self.socket.is_none() {
                    // 重试计数归零
                    self.retries = 0;
                    // 更新发现状态
                    self.update_discovery();
                    // 已有选中项
                    if self.selected.is_some() {
                        // 立即安排重试
                        self.retry_at = Some(Instant::now());
                    }
                }
            }
            // 宿主命令
            Command::Host(command) => self.host(command)?,
            // 停止由 owner 主循环处理
            Command::Stop => {}
        }
        // 发布新的公开投影
        self.publish();
        // 完成
        Ok(())
    }

    /// 用户选择才开启首次密码窗口；已信任桌面只用保存的秘密认证。
    // 用户选择桌面才开启首次配对窗口；已信任的桌面只用保存的秘密认证
    fn select(&mut self, client_id: String, revision: u64) -> Result<(), String> {
        // 桌面当前不可达
        if !self.endpoints.values().any(|e| e.id == client_id) {
            // 桌面不可达
            return Err("桌面当前不可达，请重新发现。".into());
        }
        // 是否切换了桌面
        let switched = self.selected.as_deref() != Some(&client_id);
        // 切换时小组件共享存储须先撤销；失败则不能悄悄只切换应用
        if switched {
            // 共享存储撤销失败时,不得悄悄地只切换应用本身。
            self.select_widget_source(&client_id)?;
        }
        // 取消旧连接
        self.cancel();
        // 记录选中客户端
        self.selected = Some(client_id.clone());
        // 更新视图里的选中项
        self.view["selectedId"] = json!(client_id);
        // 清除错误
        self.view["error"] = Value::Null;
        // 重试计数归零
        self.retries = 0;
        // 换桌面就清掉旧快照与显示
        if switched {
            // 删除快照缓存
            persistence::erase_snapshot(&self.root);
            // 清除显示
            self.clear_display();
        }
        // 中间选择仍须按 FIFO 生效，后续 Forget 才能忘掉正确对象；
        // 其保存资格已撤销时，只更新选择，不开启配对窗口或连接。
        // 中间选择仍按 FIFO 生效；其保存资格已撤销时只更新选择，不开启配对窗口或连接
        if revision != self.revision.load(Ordering::SeqCst) {
            // 直接返回
            return Ok(());
        }
        // 未配对的桌面需要配对码
        if !self.records.contains_key(&client_id) {
            // 配对窗口只在前台开启；后台到达的选择只保留选中项。
            // 配对窗口只在前台开启
            if !self.lifecycle.visible {
                // 直接返回
                return Ok(());
            }
            // 当前时刻
            let now = Instant::now();
            // 生成四位随机配对码
            let code = format!("{:04}", uuid::Uuid::new_v4().as_u128() % 10_000);
            // 开启配对流程
            self.protocol
                // 开始配对
                .start_pairing(&client_id, &self.name, code.clone(), revision, now)?;
            // 记录配对邀请内容
            self.pairing_offer = Some(
                // 配对信息
                json!({"kind":"pairing","value":{"version":1,"clientId":client_id,"deviceId":self.id,"name":self.name}}),
            );
            // 窗口截止时间
            self.pairing_deadline = Some(now + PAIRING_WINDOW);
            // 把配对码与截止时间写入公开视图
            self.view["pairingCode"] = json!(code);
            // 配对期限（毫秒时间戳）
            self.view["pairingDeadlineMs"] = json!(now_ms() + PAIRING_WINDOW.as_millis() as u64);
        }
        // 发起连接，失败则标记为失败状态
        if let Err(e) = self.connect() {
            // 记录失败
            self.failed(e);
        }
        // 完成
        Ok(())
    }
}

// 测试模块
#[cfg(test)]
#[path = "commands_test.rs"]
mod commands_test;
