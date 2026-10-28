//! 原键恢复、先保存后显示，以及原生名称确认属于同一个应用用例。
// 引入身份规则、面板规范化与比较空白
use crate::{
    // 默认设备名、名称校验与设备标识校验
    features::connection::identity::{DEFAULT_NAME, name_error, valid_device_id},
    // 共享契约
    shared::{
        // 面板配置的解析与恢复
        contracts::panel::{panel_from_value, restore_panel},
        // 比较用的空白判断
        domain::text::comparison_whitespace,
    },
};
// 引入序列化
use serde::{Deserialize, Serialize};
// 引入 JSON 值
use serde_json::{Value, json};

mod device;
use device::Device;

mod names;

// 面板配置的存储键
const PANEL: &str = "loki.panel";
// 设备身份的存储键
const DEVICE: &str = "loki.device";
// 名称确认的等待上限：3 秒
const NAME_CONFIRM_MS: u64 = 3_000;
// 保存失败的统一文案
const SAVE_FAILED: &str = "保存失败，请重试";

// 存储抽象，便于测试替换
pub(super) trait Store {
    // 读取
    fn get(&self, key: &str) -> Option<String>;
    // 写入
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
        // 会话编号
        session: u64,
        // 视图
        view: Value,
    },
    // 保存面板配置
    SavePanel {
        // 会话编号
        session: u64,
        // 面板
        panel: Value,
    },
    // 编辑名称草稿
    EditName {
        // 会话编号
        session: u64,
        // 草稿
        draft: String,
    },
    // 开始保存名称
    BeginNameSave {
        // 会话编号
        session: u64,
        // 草稿
        draft: String,
        // 视图
        view: Value,
        // 当前毫秒时间
        now_ms: u64,
    },
    // 观察名称是否已被连接确认
    ObserveName {
        // 会话编号
        session: u64,
        // 草稿
        draft: String,
        // 视图
        view: Value,
        // 当前毫秒时间
        now_ms: u64,
    },
    // 重命名命令投递失败
    NameDispatchFailed {
        // 会话编号
        session: u64,
    },
    // 端口调用出错
    PortFailure {
        // 会话编号
        session: u64,
        // 视图
        view: Value,
        // 失败阶段
        stage: String,
        // 错误信息
        error: String,
    },
    // 释放会话
    Dispose {
        // 会话编号
        session: u64,
    },
}

// 请求的方法
impl Request {
    // 取请求携带的会话序号，Boot 没有
    fn session(&self) -> Option<u64> {
        // 按请求种类取会话编号
        match self {
            // 启动请求没有会话
            Self::Boot => None,
            // 其余请求都带会话序号
            Self::Initialized { session, .. }
            // 以下请求都带会话编号
            | Self::SavePanel { session, .. }
            // 编辑名称
            | Self::EditName { session, .. }
            // 开始保存名称
            | Self::BeginNameSave { session, .. }
            // 观察名称
            | Self::ObserveName { session, .. }
            // 名称提交失败
            | Self::NameDispatchFailed { session }
            // 端口失败
            | Self::PortFailure { session, .. }
            // 销毁
            | Self::Dispose { session } => Some(*session),
        }
    }
}

// 正在等待确认的名称修改
struct PendingName {
    // 修改前的身份，失败时用于回滚
    previous: Device,
    // 本次尝试的身份
    attempted: Device,
    // 确认的截止时间
    deadline_ms: u64,
}

#[derive(Default)]
// 设置引擎状态
pub(super) struct Engine {
    // 会话序号，用于拒绝过期请求
    session: u64,
    // 是否已释放
    disposed: bool,
    // 启动时存储里是否已有面板配置
    panel_was_present: bool,
    // 待确认的名称修改
    pending: Option<PendingName>,
    // 名称校验问题
    name_issue: Option<String>,
    // 名称保存反馈
    name_feedback: Option<String>,
    // 面板保存问题
    panel_issue: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
// 返回给 TS 的公开状态；空字段表示无变化
pub(super) struct Response {
    // 会话编号
    session: u64,
    // 是否正在保存名称
    saving_name: bool,
    // 名称问题提示
    name_issue: Option<String>,
    // 名称保存反馈
    name_feedback: Option<String>,
    // 面板问题提示
    panel_issue: Option<String>,
    // 需要覆盖的面板配置
    panel: Option<Value>,
    // 当前设备身份
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

// 引擎的方法
impl Engine {
    // 构造一个只带当前状态的响应
    fn response(&self) -> Response {
        // 构造响应
        Response {
            // 当前会话编号
            session: self.session,
            // 有待处理保存即为保存中
            saving_name: self.pending.is_some(),
            // 名称问题
            name_issue: self.name_issue.clone(),
            // 名称反馈
            name_feedback: self.name_feedback.clone(),
            // 面板问题
            panel_issue: self.panel_issue.clone(),
            // 响应里不带面板
            panel: None,
            // 尚无设备
            device: None,
            // 尚未初始化
            initialize: None,
            // 尚无草稿
            draft: None,
            // 尚无视图
            view: None,
            // 待发命令为空
            commands: Vec::new(),
        }
    }

    // 释放：标记结束并丢弃待确认的修改
    pub(super) fn dispose(&mut self) {
        // 标记已销毁
        self.disposed = true;
        // 清除待处理保存
        self.pending = None;
    }

    // 处理一个请求
    pub(super) fn apply(
        // 自身
        &mut self,
        // 请求
        request: Request,
        // 存储
        store: &mut impl Store,
        // 返回响应
    ) -> Result<Response, String> {
        // 会话不符或已释放则拒绝
        if let Some(session) = request.session()
            // 且会话编号不符或已销毁
            && (session != self.session || self.disposed)
        {
            // 会话已关闭
            return Err("设置会话已关闭".into());
        }
        // 处理结果
        let mut result;
        // 按请求种类处理
        match request {
            // 启动：开启新会话并恢复数据
            Request::Boot => {
                // 开启新会话，编号加一
                let session = self.session.wrapping_add(1);
                // 重置引擎并换上新会话
                *self = Self {
                    // 新会话编号
                    session,
                    // 其余字段取默认值
                    ..Self::default()
                };
                // 读取面板配置
                let panel = store.get(PANEL);
                // 记录是否已存在
                self.panel_was_present = panel.is_some();
                // 恢复设备身份
                let device = Device::restored(store.get(DEVICE).as_deref());
                // 生成响应
                result = self.response();
                // 返回规范化后的面板配置、名称草稿与初始化身份
                result.panel = Some(restore_panel(panel.as_deref()));
                // 草稿取设备名
                result.draft = Some(device.name.clone());
                // 需要初始化的设备
                result.initialize = Some(device.clone());
                // 当前设备
                result.device = Some(device);
            }
            // 初始化完成：以连接投影的身份为准
            Request::Initialized { view, .. } => {
                // 由视图投影得到设备
                let device = Device::projected(&view)?;
                // 生成响应
                result = self.response();
                // 存储里没有面板配置时，采纳旧版移动端带来的配置
                if !self.panel_was_present && view["panelConfig"].is_object() {
                    // 解析面板配置
                    let panel = panel_from_value(&view["panelConfig"]);
                    // 写入存储
                    store.set(PANEL, panel.to_string())?;
                    // 面板配置已存在
                    self.panel_was_present = true;
                    // 返回面板
                    result.panel = Some(panel);
                }
                // 保存确认后的身份
                device.persist(store)?;
                // 草稿取设备名
                result.draft = Some(device.name.clone());
                // 当前设备
                result.device = Some(device);
            }
            // 保存面板：先写存储，成功才回显
            Request::SavePanel { panel, .. } => {
                // 解析面板配置
                let panel = panel_from_value(&panel);
                // 保存到存储
                match store.set(PANEL, panel.to_string()) {
                    // 成功：清除错误
                    Ok(()) => {
                        // 清除面板问题
                        self.panel_issue = None;
                        // 标记面板已存在
                        self.panel_was_present = true;
                        // 生成响应
                        result = self.response();
                        // 返回面板
                        result.panel = Some(panel.clone());
                    }
                    // 失败：给出提示，不改界面
                    Err(_) => {
                        // 提示面板设置保存失败
                        self.panel_issue = Some("面板设置保存失败，请重试".into());
                        // 生成响应
                        result = self.response();
                    }
                }
            }
            // 编辑名称：即时校验并清除旧反馈
            Request::EditName { draft, .. } => {
                // 校验名称并记录问题
                self.name_issue = name_error(&draft).map(str::to_owned);
                // 清除名称反馈
                self.name_feedback = None;
                // 生成响应
                result = self.response();
            }
            // 开始保存名称
            Request::BeginNameSave {
                // 草稿
                draft,
                // 视图
                view,
                // 当前毫秒时间
                now_ms,
                // 其余字段忽略
                ..
            // 分支开始
            } => {
                // 开始保存名称
                result = self.begin_name(draft, view, now_ms, store);
            }
            // 观察确认
            Request::ObserveName {
                // 草稿
                draft,
                // 视图
                view,
                // 当前毫秒时间
                now_ms,
                // 其余字段忽略
                ..
            // 分支开始
            } => {
                // 观察名称保存结果
                let normalized = self.observe_name(&draft, &view, now_ms, store);
                // 生成响应
                result = self.response();
                // 草稿取规范化后的值
                result.draft = normalized;
            }
            // 命令投递失败：回滚
            Request::NameDispatchFailed { .. } => {
                // 名称保存失败的处理
                self.fail_name(store);
                // 生成响应
                result = self.response();
            }
            // 端口出错：把视图置为未连接并给出原因
            Request::PortFailure {
                // 可变视图
                mut view,
                // 失败阶段
                stage,
                // 错误信息
                error,
                // 其余字段忽略
                ..
            // 分支开始
            } => {
                // 视图必须是对象
                if !view.is_object() {
                    // 否则报连接投影无效
                    return Err("连接投影无效".into());
                }
                // 轮询阶段
                if stage == "poll" {
                    // 原生投影读失败后无法继续确认名称，结束这次保存并恢复自己的记录。
                    // 轮询失败后无法继续确认名称，结束这次保存
                    self.fail_name(store);
                }
                // 标记未连接
                view["connected"] = json!(false);
                // 标记非实时
                view["live"] = json!(false);
                // 写入错误信息
                view["error"] = json!(error);
                // 按阶段给出状态文案
                view["status"] = json!(match stage.as_str() {
                    // 初始化阶段的错误文案
                    "initialize" => "连接服务不可用",
                    // 轮询阶段
                    "poll" => "同步失败",
                    // 设置阶段
                    "settings" => "本机设置不可用",
                    // 其余阶段
                    _ => "连接操作失败",
                });
                // 生成响应
                result = self.response();
                // 返回视图
                result.view = Some(view);
            }
            // 释放
            Request::Dispose { .. } => {
                // 销毁会话
                self.dispose();
                // 生成响应
                result = self.response();
            }
        }
        // 返回响应
        Ok(result)
    }
}

// 测试模块
#[cfg(test)]
#[path = "engine_test.rs"]
mod engine_test;
