//! Private WidgetKit summaries, read-only grants and revocations follow owner event ordering.
use super::{now_ms, owner::Owner};
use loki_widget_core::{store::Store, summary, widget_http};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct WidgetShare {
    pub(super) store: Option<Store>,
    authenticated: bool,
}

impl WidgetShare {
    pub(super) fn new(directory: Option<PathBuf>) -> Self {
        Self {
            store: directory.filter(|path| path.is_absolute()).map(Store::new),
            authenticated: false,
        }
    }
}

impl Owner {
    pub(super) fn reconcile_widget_authorization(&self) {
        let Some(store) = &self.widget.store else {
            return;
        };
        let allowed = |source: &str| {
            self.records
                .get(source)
                .is_some_and(|record| record.device_id == self.id)
        };
        let target_valid = store.load_target().ok().flatten().is_none_or(|target| {
            target["deviceId"] == self.id && target["sourceId"].as_str().is_some_and(allowed)
        });
        let summary_valid = store
            .load()
            .ok()
            .flatten()
            .is_none_or(|record| record["sourceId"].as_str().is_some_and(allowed));
        if (!target_valid || !summary_valid) && store.clear().is_ok() {
            crate::native::widgets::changed();
        }
    }

    /// Only invoked after a full, authenticated snapshot passed admission.
    pub(super) fn share_widget_snapshot(&mut self, message: &Value, received: u64) {
        let (Some(store), Some(source)) = (&self.widget.store, self.selected.as_deref()) else {
            return;
        };
        if !self.records.contains_key(source) {
            return;
        }
        let Some(record) = i64::try_from(received)
            .ok()
            .and_then(|at| summary::from_message(message, source, at))
        else {
            return;
        };
        let target = self.socket.as_ref().and_then(|socket| {
            widget_http::target_from_message(
                message,
                source,
                &self.id,
                &socket.peer.ip().to_string(),
            )
        });
        if store.save_authenticated(&record, target.as_ref()).is_ok() {
            self.widget.authenticated = true;
            crate::native::widgets::changed();
        }
    }

    /// A heartbeat renews the lease but does not invent a newer received timestamp or reload.
    pub(super) fn renew_widget_lease(&self) {
        if !self.widget.authenticated {
            return;
        }
        if let (Some(store), Some(source), Ok(now)) = (
            &self.widget.store,
            self.selected.as_deref(),
            i64::try_from(now_ms()),
        ) {
            let _ = store.renew(source, now);
        }
    }

    /// Disconnecting stops lease renewal; the independent read grant remains authorized.
    pub(super) fn pause_widget_lease(&mut self) {
        self.widget.authenticated = false;
    }

    pub(super) fn select_widget_source(&mut self, source: &str) -> Result<(), String> {
        if let Some(store) = &self.widget.store {
            store.select(source)?;
            crate::native::widgets::changed();
        }
        self.widget.authenticated = false;
        Ok(())
    }

    pub(super) fn forget_widget_source(&mut self, source: &str) -> Result<(), String> {
        if let Some(store) = &self.widget.store {
            store.forget(source)?;
            crate::native::widgets::changed();
        }
        if self.selected.as_deref() == Some(source) {
            self.widget.authenticated = false;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "widget_test.rs"]
mod widget_test;
