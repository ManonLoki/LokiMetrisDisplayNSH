//! 原键恢复、先保存后显示，以及原生名称确认属于同一个应用用例。
use crate::{
    // 默认设备名、名称校验与设备标识校验
    features::connection::identity::{DEFAULT_NAME, name_error, valid_device_id},
    shared::{
        // 面板配置的解析与恢复
        contracts::panel::{panel_from_value, restore_panel},
        domain::text::comparison_whitespace,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod device;
use device::Device;

mod names;

const PANEL: &str = "loki.panel";
const DEVICE: &str = "loki.device";
// 名称确认的等待上限：3 秒
const NAME_CONFIRM_MS: u64 = 3_000;
// 保存失败的统一文案
const SAVE_FAILED: &str = "保存失败，请重试";

// 存储抽象，便于测试替换
pub(super) trait Store {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&mut self, key: &str, value: String) -> Result<(), String>;
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
// TS 发来的请求，以 op 区分
pub(super) enum Request {
    // 启动：恢复面板与设备身份
    Boot,
    // 原生端口初始化完成
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
    // 观察名称是否已被连接确认
    ObserveName {
        session: u64,
        draft: String,
        view: Value,
        now_ms: u64,
    },
    // 重命名命令投递失败
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
    // 取请求携带的会话序号，Boot 没有
    fn session(&self) -> Option<u64> {
        // 按请求种类取会话编号
        match self {
            Self::Boot => None,
            // 其余请求都带会话序号
            Self::Initialized { session, .. }
            // 以下请求都带会话编号
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

// 正在等待确认的名称修改
struct PendingName {
    // 修改前的身份，失败时用于回滚
    previous: Device,
    attempted: Device,
    deadline_ms: u64,
}

#[derive(Default)]
pub(super) struct Engine {
    // 会话序号，用于拒绝过期请求
    session: u64,
    disposed: bool,
    pending: Option<PendingName>,
    name_issue: Option<String>,
    name_feedback: Option<String>,
    panel_issue: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
// 返回给 TS 的公开状态；空字段表示无变化
pub(super) struct Response {
    session: u64,
    saving_name: bool,
    name_issue: Option<String>,
    name_feedback: Option<String>,
    panel_issue: Option<String>,
    // 需要覆盖的面板配置
    panel: Option<Value>,
    device: Option<Device>,
    // 用于初始化原生端口的身份
    initialize: Option<Device>,
    // 需要覆盖的名称草稿
    draft: Option<String>,
    // 需要覆盖的连接视图
    view: Option<Value>,
    // 需要下发给端口的命令
    commands: Vec<Value>,
}

impl Engine {
    pub(super) fn name_confirmation_deadline(&self) -> Option<u64> {
        self.pending.as_ref().map(|name| name.deadline_ms)
    }

    // 构造一个只带当前状态的响应
    fn response(&self) -> Response {
        Response {
            session: self.session,
            // 有待处理保存即为保存中
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

    // 释放：标记结束并丢弃待确认的修改
    pub(super) fn dispose(&mut self) {
        self.disposed = true;
        self.pending = None;
    }

    pub(super) fn apply(
        &mut self,
        request: Request,
        store: &mut impl Store,
    ) -> Result<Response, String> {
        // 会话不符或已释放则拒绝
        if let Some(session) = request.session()
            // 且会话编号不符或已销毁
            && (session != self.session || self.disposed)
        {
            return Err("设置会话已关闭".into());
        }
        let mut result;
        match request {
            // 启动：开启新会话并恢复数据
            Request::Boot => {
                // 开启新会话，编号加一
                let session = self.session.wrapping_add(1);
                // 重置引擎并换上新会话
                *self = Self {
                    session,
                    ..Self::default()
                };
                let panel = store.get(PANEL);
                let device = Device::restored(store.get(DEVICE).as_deref());
                result = self.response();
                // 返回规范化后的面板配置、名称草稿与初始化身份
                result.panel = Some(restore_panel(panel.as_deref()));
                result.draft = Some(device.name.clone());
                result.initialize = Some(device.clone());
                result.device = Some(device);
            }
            // 初始化完成：以连接投影的身份为准
            Request::Initialized { view, .. } => {
                // 由视图投影得到设备
                let device = Device::projected(&view)?;
                result = self.response();
                device.persist(store)?;
                result.draft = Some(device.name.clone());
                result.device = Some(device);
            }
            // 保存面板：先写存储，成功才回显
            Request::SavePanel { panel, .. } => {
                let panel = panel_from_value(&panel);
                match store.set(PANEL, panel.to_string()) {
                    Ok(()) => {
                        self.panel_issue = None;
                        result = self.response();
                        result.panel = Some(panel.clone());
                    }
                    // 失败：给出提示，不改界面
                    Err(_) => {
                        // 提示面板设置保存失败
                        self.panel_issue = Some("面板设置保存失败，请重试".into());
                        result = self.response();
                    }
                }
            }
            // 编辑名称：即时校验并清除旧反馈
            Request::EditName { draft, .. } => {
                // 校验名称并记录问题
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
                // 草稿取规范化后的值
                result.draft = normalized;
            }
            // 命令投递失败：回滚
            Request::NameDispatchFailed { .. } => {
                // 名称保存失败的处理
                self.fail_name(store);
                result = self.response();
            }
            // 端口出错：把视图置为未连接并给出原因
            Request::PortFailure {
                mut view,
                stage,
                error,
                ..
            } => {
                // 视图必须是对象
                if !view.is_object() {
                    // 否则报连接投影无效
                    return Err("连接投影无效".into());
                }
                if stage == "poll" {
                    // 原生投影读失败后无法继续确认名称，结束这次保存并恢复自己的记录。
                    // 轮询失败后无法继续确认名称，结束这次保存
                    self.fail_name(store);
                }
                view["connected"] = json!(false);
                view["live"] = json!(false);
                view["error"] = json!(error);
                // 按阶段给出状态文案
                view["status"] = json!(match stage.as_str() {
                    // 初始化阶段的错误文案
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
}

#[cfg(test)]
#[path = "engine_test.rs"]
mod engine_test;
