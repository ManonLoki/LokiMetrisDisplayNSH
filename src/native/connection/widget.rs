//! 私有 WidgetKit 摘要、只读授权与撤销,遵循所有者的事件顺序。
// 引入时间与 owner
use super::{now_ms, owner::Owner};
// 引入小组件存储、摘要与 HTTP 目标
use loki_widget_core::{store::Store, summary, widget_http};
// 引入 JSON
use serde_json::Value;
// 引入路径
use std::path::PathBuf;

#[derive(Default)]
// 小组件共享状态
pub(super) struct WidgetShare {
    // 共享存储，没有 App Group 时为空
    pub(super) store: Option<Store>,
    // 当前连接是否已通过认证
    authenticated: bool,
}

// 小组件共享的方法
impl WidgetShare {
    // 仅接受绝对路径的目录
    pub(super) fn new(directory: Option<PathBuf>) -> Self {
        // 构造小组件共享
        Self {
            // 目录须是绝对路径才创建存储
            store: directory.filter(|path| path.is_absolute()).map(Store::new),
            // 尚未认证
            authenticated: false,
        }
    }
}

// 所有者的方法
impl Owner {
    // 对账：共享存储里的目标与摘要必须仍对应一份本机的配对记录，否则清除
    pub(super) fn reconcile_widget_authorization(&self) {
        // 没有共享存储则无事可做
        let Some(store) = &self.widget.store else {
            // 直接返回
            return;
        };
        // 来源有本机配对记录且设备 ID 一致才算授权
        let allowed = |source: &str| {
            // 配对记录
            self.records
                // 按来源取记录
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
            // 读取当前摘要
            .load()
            // 出错忽略
            .ok()
            // 展平
            .flatten()
            // 没有摘要，或者其来源被允许
            .is_none_or(|record| record["sourceId"].as_str().is_some_and(allowed));
        // 任一失效则清空共享存储并刷新小组件
        if (!target_valid || !summary_valid) && store.clear().is_ok() {
            // 通知小组件有变化
            crate::native::widgets::changed();
        }
    }

    /// 仅在完整且已认证的快照通过准入之后才会调用。
    // 只在完整且通过准入的认证快照之后调用
    pub(super) fn share_widget_snapshot(&mut self, message: &Value, received: u64) {
        // 需要共享存储与当前选中的桌面
        let (Some(store), Some(source)) = (&self.widget.store, self.selected.as_deref()) else {
            // 直接返回
            return;
        };
        // 来源必须是已配对的桌面
        if !self.records.contains_key(source) {
            // 直接返回
            return;
        }
        // 生成摘要记录
        let Some(record) = i64::try_from(received)
            // 出错忽略
            .ok()
            // 由消息构造摘要
            .and_then(|at| summary::from_message(message, source, at))
        // 否则
        else {
            // 直接返回
            return;
        };
        // 取得小组件直连桌面的 HTTP 目标
        let target = self.socket.as_ref().and_then(|socket| {
            // 由消息构造凭据
            widget_http::target_from_message(
                // 消息
                message,
                // 来源
                source,
                // 本机标识
                &self.id,
                // 对端地址
                &socket.peer.ip().to_string(),
            )
        });
        // 保存成功才标记已认证并刷新小组件
        if store.save_authenticated(&record, target.as_ref()).is_ok() {
            // 标记已认证
            self.widget.authenticated = true;
            // 通知小组件有变化
            crate::native::widgets::changed();
        }
    }

    /// 心跳会续租,但不会凭空制造更新的接收时间戳或重新加载。
    // 心跳只续租，不伪造更新的接收时间，也不触发刷新
    pub(super) fn renew_widget_lease(&self) {
        // 未认证
        if !self.widget.authenticated {
            // 直接返回
            return;
        }
        // 存储、来源与当前时间都可用
        if let (Some(store), Some(source), Ok(now)) = (
            // 存储
            &self.widget.store,
            // 选中项
            self.selected.as_deref(),
            // 当前时间
            i64::try_from(now_ms()),
            // 参数结束
        ) {
            // 续租，失败忽略
            let _ = store.renew(source, now);
        }
    }

    /// 断开连接会停止续租;独立的读取授权仍然有效。
    // 断开后停止续租；独立的读授权保持有效
    pub(super) fn pause_widget_lease(&mut self) {
        // 标记未认证
        self.widget.authenticated = false;
    }

    // 切换来源：共享存储同步切换并刷新，认证状态重置
    pub(super) fn select_widget_source(&mut self, source: &str) -> Result<(), String> {
        // 有存储时
        if let Some(store) = &self.widget.store {
            // 选中该来源
            store.select(source)?;
            // 通知小组件有变化
            crate::native::widgets::changed();
        }
        // 标记未认证
        self.widget.authenticated = false;
        // 完成
        Ok(())
    }

    // 忘记来源：撤销授权并刷新；忘记的是当前来源则重置认证状态
    pub(super) fn forget_widget_source(&mut self, source: &str) -> Result<(), String> {
        // 有存储时
        if let Some(store) = &self.widget.store {
            // 忘记该来源
            store.forget(source)?;
            // 通知小组件有变化
            crate::native::widgets::changed();
        }
        // 被忘记的正是选中项
        if self.selected.as_deref() == Some(source) {
            // 标记未认证
            self.widget.authenticated = false;
        }
        // 完成
        Ok(())
    }
}

// 测试模块
#[cfg(test)]
#[path = "widget_test.rs"]
mod widget_test;
