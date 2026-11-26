//! 句柄提交有界命令，取消代数随入队意图更新。
use super::{Command, Event, Handle};
use std::sync::atomic::Ordering;

/// revision 随取消意图提交，OS 同步存储尚未返回时也能撤销保存 ACK。
// 已入队的命令：随取消代数提交，OS 同步存储尚未返回时也能撤销保存确认
pub(super) struct QueuedCommand {
    pub(super) command: Command,
    pub(super) revision: u64,
}
/// 记录成功入队的选择意图。存储阻塞时，公开 view 可能还停留在上一个桌面。
#[derive(Default)]
// 成功入队的选择意图；存储阻塞时公开 view 可能还停留在上一个桌面
pub(super) struct CommandIntent {
    // 当前意图选中的桌面
    pub(super) selected: Option<String>,
    pub(super) stopped: bool,
}
impl Handle {
    // 入队命令；停止后拒绝
    pub(super) fn enqueue(&self, command: Command) -> Result<(), String> {
        let mut intent = self.intent.lock().unwrap_or_else(|p| p.into_inner());
        if intent.stopped {
            return Err("连接已关闭".into());
        }
        // 选择、置为不可见、停止，以及忘记当前选中的桌面，都会中断进行中的配对
        let interrupts = match &command {
            // 选择、停用或停止命令会使旧修订失效
            Command::Select { .. } | Command::Active { value: false } | Command::Stop => true,
            // 忘记命令且正是当前选中项
            Command::Forget { client_id } => intent.selected.as_ref() == Some(client_id),
            _ => false,
        };
        // 中断类命令递增取消代数；溢出则报错
        let revision = if interrupts {
            self.revision
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| "配对取消状态已用尽，请重新启动应用。")?
                + 1
        // 否则分支
        } else {
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
            _ => None,
        };
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
            intent.selected = selected;
        }
        intent.stopped = stopped;
        Ok(())
    }
}
