//! 私有 WidgetKit 摘要、只读授权与撤销,遵循所有者的事件顺序。
use super::{now_ms, owner::Owner};
use loki_widget_core::{store::Store, summary, widget_http};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct WidgetShare {
    // 共享存储，没有 App Group 时为空
    pub(super) store: Option<Store>,
    // 当前连接是否已通过认证
    authenticated: bool,
}

impl WidgetShare {
    // 仅接受绝对路径的目录
    pub(super) fn new(directory: Option<PathBuf>) -> Self {
        Self {
            // 目录须是绝对路径才创建存储
            store: directory.filter(|path| path.is_absolute()).map(Store::new),
            authenticated: false,
        }
    }
}

impl Owner {
    // 对账：共享存储里的目标与摘要必须仍对应一份本机的配对记录，否则清除
    pub(super) fn reconcile_widget_authorization(&self) {
        // 没有共享存储则无事可做
        let Some(store) = &self.widget.store else {
            return;
        };
        // 来源有本机配对记录且设备 ID 一致才算授权
        let allowed = |source: &str| {
            self.records
                .get(source)
                // 设备标识须与本机一致
                .is_some_and(|record| record.device_id == self.id)
        };
        // 读取目标授权是否仍有效
        let target_valid = store.load_target().ok().flatten().is_none_or(|target| {
            // 凭据属于本设备且来源被允许
            target["deviceId"] == self.id && target["sourceId"].as_str().is_some_and(allowed)
        });
        // 读取摘要是否仍有效
        let summary_valid = store
            .load()
            .ok()
            .flatten()
            // 没有摘要，或者其来源被允许
            .is_none_or(|record| record["sourceId"].as_str().is_some_and(allowed));
        // 任一失效则清空共享存储并刷新小组件
        if (!target_valid || !summary_valid) && store.clear().is_ok() {
            crate::native::widgets::changed();
        }
    }

    /// 仅在完整且已认证的快照通过准入之后才会调用。
    // 只在完整且通过准入的认证快照之后调用
    pub(super) fn share_widget_snapshot(&mut self, message: &Value, received: u64) {
        // 需要共享存储与当前选中的桌面
        let (Some(store), Some(source)) = (&self.widget.store, self.selected.as_deref()) else {
            return;
        };
        // 来源必须是已配对的桌面
        if !self.records.contains_key(source) {
            return;
        }
        let Some(record) = i64::try_from(received)
            .ok()
            .and_then(|at| summary::from_message(message, source, at))
        // 否则
        else {
            return;
        };
        // 取得小组件直连桌面的 HTTP 目标
        let target = self.socket.as_ref().and_then(|socket| {
            widget_http::target_from_message(
                message,
                source,
                &self.id,
                &socket.peer.ip().to_string(),
            )
        });
        // 保存成功才标记已认证并刷新小组件
        if store.save_authenticated(&record, target.as_ref()).is_ok() {
            self.widget.authenticated = true;
            crate::native::widgets::changed();
            // 首条 Hook（槽位最小）同步到灵动岛；无 Hook 时撤下
            crate::native::widgets::agent_activity(&lead_activity_payload(message, received));
        }
    }

    /// 心跳会续租,但不会凭空制造更新的接收时间戳或重新加载。
    // 心跳只续租，不伪造更新的接收时间，也不触发刷新
    pub(super) fn renew_widget_lease(&self) {
        if !self.widget.authenticated {
            return;
        }
        // 存储、来源与当前时间都可用
        if let (Some(store), Some(source), Ok(now)) = (
            &self.widget.store,
            self.selected.as_deref(),
            i64::try_from(now_ms()),
        ) {
            let _ = store.renew(source, now);
        }
    }

    /// 断开连接会停止续租;独立的读取授权仍然有效。
    // 断开后停止续租；独立的读授权保持有效
    pub(super) fn pause_widget_lease(&mut self) {
        self.widget.authenticated = false;
        // 撤下灵动岛的 Agent 状态
        crate::native::widgets::agent_activity(&serde_json::json!({"present": false}).to_string());
    }

    // 切换来源：共享存储同步切换并刷新，认证状态重置
    pub(super) fn select_widget_source(&mut self, source: &str) -> Result<(), String> {
        if let Some(store) = &self.widget.store {
            store.select(source)?;
            crate::native::widgets::changed();
        }
        self.widget.authenticated = false;
        Ok(())
    }

    // 忘记来源：撤销授权并刷新；忘记的是当前来源则重置认证状态
    pub(super) fn forget_widget_source(&mut self, source: &str) -> Result<(), String> {
        if let Some(store) = &self.widget.store {
            store.forget(source)?;
            crate::native::widgets::changed();
            crate::native::widgets::agent_activity(
                &serde_json::json!({"present": false}).to_string(),
            );
        }
        // 被忘记的正是选中项
        if self.selected.as_deref() == Some(source) {
            self.widget.authenticated = false;
        }
        Ok(())
    }
}

/// 由认证快照提取首条 Hook（槽位最小）的灵动岛状态；
/// 内容与桌面桌宠当前页同源，缺失或为空时撤下活动。
fn lead_activity_payload(message: &serde_json::Value, received: u64) -> String {
    // 快照来源与 lead_activity 的取数一致
    let snapshot = if message["type"] == "snapshot" {
        &message["snapshot"]
    } else {
        message
    };
    // 槽位最小的 Hook 作为灵动岛状态
    let lead = snapshot["hooks"]
        .as_array()
        .and_then(|hooks| hooks.iter().min_by_key(|hook| hook["slot"].as_u64()));
    // 没有 Hook 或行为缺失则撤下
    let Some(lead) = lead.filter(|lead| !lead["agent"].is_null() && !lead["behavior"].is_null())
    else {
        return serde_json::json!({"present": false}).to_string();
    };
    // 组装载荷：展示名、行为、文案与接收时间
    serde_json::json!({
        "present": true,
        "agent": crate::shared::domain::agents::agent_display_name(lead["agent"].as_str().unwrap_or_default()),
        "behavior": lead["behavior"].as_str().unwrap_or_default(),
        "content": lead["content"].as_str().unwrap_or_default(),
        "receivedAtMs": received,
    })
    .to_string()
}

#[cfg(test)]
#[path = "widget_test.rs"]
mod widget_test;
