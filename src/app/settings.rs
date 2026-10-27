//! 原键恢复、先保存后显示，以及原生名称确认属于同一个应用用例。
use crate::{
    features::connection::identity::{name_error, valid_device_id},
    shared::{
        contracts::panel::{panel_from_value, restore_panel},
        domain::agents::comparison_whitespace,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const PANEL: &str = "loki.panel";
const DEVICE: &str = "loki.device";
const FALLBACK_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const DEFAULT_NAME: &str = "LokiMetris Display";
const NAME_CONFIRM_MS: u64 = 3_000;
const SAVE_FAILED: &str = "保存失败，请重试";

pub(super) trait Store {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&mut self, key: &str, value: String) -> Result<(), String>;
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(super) enum Request {
    Boot,
    Initialized {
        session: u64,
        view: Value,
    },
    SavePanel {
        session: u64,
        panel: Value,
    },
    EditName {
        session: u64,
        draft: String,
    },
    BeginNameSave {
        session: u64,
        draft: String,
        view: Value,
        now_ms: u64,
    },
    ObserveName {
        session: u64,
        draft: String,
        view: Value,
        now_ms: u64,
    },
    NameDispatchFailed {
        session: u64,
    },
    PortFailure {
        session: u64,
        view: Value,
        stage: String,
        error: String,
    },
    Dispose {
        session: u64,
    },
}

impl Request {
    fn session(&self) -> Option<u64> {
        match self {
            Self::Boot => None,
            Self::Initialized { session, .. }
            | Self::SavePanel { session, .. }
            | Self::EditName { session, .. }
            | Self::BeginNameSave { session, .. }
            | Self::ObserveName { session, .. }
            | Self::NameDispatchFailed { session }
            | Self::PortFailure { session, .. }
            | Self::Dispose { session } => Some(*session),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct Device {
    id: String,
    name: String,
}

impl Device {
    fn restored(raw: Option<&str>) -> Self {
        let value = raw
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .unwrap_or(Value::Null);
        Self {
            id: value["id"]
                .as_str()
                .filter(|id| valid_device_id(id))
                .unwrap_or(FALLBACK_ID)
                .into(),
            name: value["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .unwrap_or(DEFAULT_NAME)
                .into(),
        }
    }

    fn projected(view: &Value) -> Result<Self, String> {
        let id = view["deviceId"]
            .as_str()
            .filter(|id| valid_device_id(id))
            .ok_or("设备身份不可用")?;
        let name = view["deviceName"].as_str().ok_or("设备名称不可用")?;
        Ok(Self {
            id: id.into(),
            name: name.into(),
        })
    }

    fn persist(&self, store: &mut impl Store) -> Result<(), String> {
        store.set(
            DEVICE,
            serde_json::to_string(self).map_err(|error| error.to_string())?,
        )
    }
}

struct PendingName {
    previous: Device,
    attempted: Device,
    deadline_ms: u64,
}

#[derive(Default)]
pub(super) struct Engine {
    session: u64,
    disposed: bool,
    panel_was_present: bool,
    pending: Option<PendingName>,
    name_issue: Option<String>,
    name_feedback: Option<String>,
    panel_issue: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Response {
    session: u64,
    saving_name: bool,
    name_issue: Option<String>,
    name_feedback: Option<String>,
    panel_issue: Option<String>,
    panel: Option<Value>,
    device: Option<Device>,
    initialize: Option<Device>,
    draft: Option<String>,
    view: Option<Value>,
    commands: Vec<Value>,
}

impl Engine {
    fn response(&self) -> Response {
        Response {
            session: self.session,
            saving_name: self.pending.is_some(),
            name_issue: self.name_issue.clone(),
            name_feedback: self.name_feedback.clone(),
            panel_issue: self.panel_issue.clone(),
            panel: None,
            device: None,
            initialize: None,
            draft: None,
            view: None,
            commands: Vec::new(),
        }
    }

    pub(super) fn dispose(&mut self) {
        self.disposed = true;
        self.pending = None;
    }

    pub(super) fn apply(
        &mut self,
        request: Request,
        store: &mut impl Store,
    ) -> Result<Response, String> {
        if let Some(session) = request.session()
            && (session != self.session || self.disposed)
        {
            return Err("设置会话已关闭".into());
        }
        let mut result;
        match request {
            Request::Boot => {
                let session = self.session.wrapping_add(1);
                *self = Self {
                    session,
                    ..Self::default()
                };
                let panel = store.get(PANEL);
                self.panel_was_present = panel.is_some();
                let device = Device::restored(store.get(DEVICE).as_deref());
                result = self.response();
                result.panel = Some(restore_panel(panel.as_deref()));
                result.draft = Some(device.name.clone());
                result.initialize = Some(Device {
                    id: if device.id == FALLBACK_ID {
                        String::new()
                    } else {
                        device.id.clone()
                    },
                    name: device.name.clone(),
                });
                result.device = Some(device);
            }
            Request::Initialized { view, .. } => {
                let device = Device::projected(&view)?;
                result = self.response();
                if !self.panel_was_present && view["panelConfig"].is_object() {
                    let panel = panel_from_value(&view["panelConfig"]);
                    store.set(PANEL, panel.to_string())?;
                    self.panel_was_present = true;
                    result.panel = Some(panel);
                }
                device.persist(store)?;
                result.draft = Some(device.name.clone());
                result.device = Some(device);
            }
            Request::SavePanel { panel, .. } => {
                let panel = panel_from_value(&panel);
                match store.set(PANEL, panel.to_string()) {
                    Ok(()) => {
                        self.panel_issue = None;
                        self.panel_was_present = true;
                        result = self.response();
                        result.panel = Some(panel.clone());
                    }
                    Err(_) => {
                        self.panel_issue = Some("面板设置保存失败，请重试".into());
                        result = self.response();
                    }
                }
            }
            Request::EditName { draft, .. } => {
                self.name_issue = name_error(&draft).map(str::to_owned);
                self.name_feedback = None;
                result = self.response();
            }
            Request::BeginNameSave {
                draft,
                view,
                now_ms,
                ..
            } => {
                result = self.begin_name(draft, view, now_ms, store);
            }
            Request::ObserveName {
                draft,
                view,
                now_ms,
                ..
            } => {
                let normalized = self.observe_name(&draft, &view, now_ms, store);
                result = self.response();
                result.draft = normalized;
            }
            Request::NameDispatchFailed { .. } => {
                self.fail_name(store);
                result = self.response();
            }
            Request::PortFailure {
                mut view,
                stage,
                error,
                ..
            } => {
                if !view.is_object() {
                    return Err("连接投影无效".into());
                }
                if stage == "poll" {
                    // 原生投影读失败后无法继续确认名称，结束这次保存并恢复自己的记录。
                    self.fail_name(store);
                }
                view["connected"] = json!(false);
                view["live"] = json!(false);
                view["error"] = json!(error);
                view["status"] = json!(match stage.as_str() {
                    "initialize" => "连接服务不可用",
                    "poll" => "同步失败",
                    "settings" => "本机设置不可用",
                    _ => "连接操作失败",
                });
                result = self.response();
                result.view = Some(view);
            }
            Request::Dispose { .. } => {
                self.dispose();
                result = self.response();
            }
        }
        Ok(result)
    }

    fn begin_name(
        &mut self,
        draft: String,
        view: Value,
        now_ms: u64,
        store: &mut impl Store,
    ) -> Response {
        if self.pending.is_some() {
            return self.response();
        }
        let previous = Device::projected(&view);
        self.name_issue = name_error(&draft).map(str::to_owned);
        if previous.is_err() && self.name_issue.is_none() {
            self.name_issue = Some(SAVE_FAILED.into());
        }
        if self.name_issue.is_some() {
            self.name_feedback = Some(SAVE_FAILED.into());
            return self.response();
        }
        let previous = previous.expect("validated device");
        let attempted = Device {
            id: previous.id.clone(),
            name: draft,
        };
        if attempted.persist(store).is_err() {
            self.name_feedback = Some(SAVE_FAILED.into());
            return self.response();
        }
        let command = json!({"op":"rename", "name":attempted.name});
        self.pending = Some(PendingName {
            previous,
            attempted,
            deadline_ms: now_ms.saturating_add(NAME_CONFIRM_MS),
        });
        let mut result = self.response();
        result.commands.push(command);
        result
    }

    fn observe_name(
        &mut self,
        draft: &str,
        view: &Value,
        now_ms: u64,
        store: &mut impl Store,
    ) -> Option<String> {
        let pending = self.pending.as_ref()?;
        let current = Device::projected(view).ok();
        if current.as_ref() == Some(&pending.attempted) {
            let name = pending.attempted.name.clone();
            self.pending = None;
            // 确认旧请求时不覆盖期间继续输入的草稿、校验或反馈。
            if draft == name || draft.trim_matches(comparison_whitespace) == name {
                self.name_issue = None;
                self.name_feedback = Some("已保存，下次连接生效".into());
                return Some(name);
            }
        } else if now_ms >= pending.deadline_ms
            || current.as_ref().is_none_or(|d| d.id != pending.previous.id)
        {
            self.fail_name(store);
        }
        None
    }

    fn fail_name(&mut self, store: &mut impl Store) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        self.name_feedback = Some(SAVE_FAILED.into());
        // 另一个窗口可能已保存更新值，旧请求只能回滚自己仍占有的记录。
        let stored = store
            .get(DEVICE)
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
        if stored.as_ref() == Some(&json!(pending.attempted)) {
            let _ = pending.previous.persist(store);
        }
    }
}

#[cfg(test)]
#[path = "settings_test.rs"]
mod settings_test;
