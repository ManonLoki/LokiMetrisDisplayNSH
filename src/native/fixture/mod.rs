//! 调试构建专用的测试夹具连接:模拟桌面端的公开投影,不实现密码协议、网络或系统凭据。
//! 真实协议由 Rust 的配对核心与实网专项验证;这里只让 UI 场景测试可重复。
//! 时间全部由调用方传入(测试里是虚拟时钟),夹具自己从不读取系统时间。
// 事件时间线场景
mod scenario;
// 读取夹具、配对记录和缓存
mod load;

use crate::features::connection::view_state::native_view;
use crate::shared::contracts::models::{LinkCommand, LinkOp, ViewState, typed};
use nsh_core::storage::Storage;
use scenario::Scenario;
use serde_json::{Value, json};

// 事件夹具的存储键
const INJECT_KEY: &str = "loki.inject";
// 固定投影夹具的存储键
const STATIC_KEY: &str = "test.staticView";
// 固定投影夹具记录命令的存储键
const COMMANDS_KEY: &str = "test.connectionCommands";

// 测试夹具连接：事件场景或固定投影
pub(super) enum Fixture {
    // 按事件时间线推进的桌面端场景
    Scenario(Scenario),
    // 只返回固定投影并记录命令
    Static(StaticView),
}

impl Fixture {
    // 按存储里的夹具键选择；都没有时用真实连接
    pub(super) fn select() -> Option<Self> {
        if let Some(raw) = Storage::get_item(STATIC_KEY) {
            return Some(Self::Static(StaticView::new(&raw)));
        }
        Storage::get_item(INJECT_KEY).map(|raw| Self::Scenario(Scenario::load(&raw)))
    }

    // 初始化并返回首个公开视图
    pub(super) fn initialize(
        &mut self,
        device_id: &str,
        name: &str,
        now_ms: u64,
    ) -> Result<ViewState, String> {
        match self {
            Self::Scenario(scenario) => scenario.initialize(device_id, name, now_ms),
            Self::Static(view) => view.initialize(),
        }
    }

    // 轮询：没有变化时为 None
    pub(super) fn poll(&mut self, now_ms: u64) -> Result<Option<ViewState>, String> {
        match self {
            Self::Scenario(scenario) => scenario.poll(now_ms),
            Self::Static(view) => view.poll(),
        }
    }

    // 处理命令
    pub(super) fn command(&mut self, command: &LinkCommand, now_ms: u64) {
        match self {
            Self::Scenario(scenario) => scenario.command(command, now_ms),
            Self::Static(view) => view.command(command),
        }
    }
}

// 只在文本变化时交付新视图的去重器
#[derive(Default)]
pub(super) struct ChangedOnly(Option<String>);

impl ChangedOnly {
    // 与产品 Link.poll 一样：先校验，再记录
    pub(super) fn view(&mut self, raw: String) -> Result<Option<ViewState>, String> {
        if self.0.as_deref() == Some(raw.as_str()) {
            return Ok(None);
        }
        let view = typed(native_view(&raw)?)?;
        self.0 = Some(raw);
        Ok(Some(view))
    }
}

// 固定投影夹具
pub(super) struct StaticView {
    // 投影
    projection: Value,
    // 收到的命令
    commands: Vec<Value>,
    // 去重器
    changed: ChangedOnly,
}

impl StaticView {
    // 读取固定投影
    fn new(raw: &str) -> Self {
        Self {
            projection: serde_json::from_str(raw).unwrap_or(Value::Null),
            commands: Vec::new(),
            changed: ChangedOnly::default(),
        }
    }

    // 首个视图
    fn initialize(&mut self) -> Result<ViewState, String> {
        self.changed
            .view(self.projection.to_string())?
            .ok_or_else(|| "夹具投影无效".to_owned())
    }

    // 轮询
    fn poll(&mut self) -> Result<Option<ViewState>, String> {
        self.changed.view(self.projection.to_string())
    }

    // 记录命令；重命名时同步修改投影
    fn command(&mut self, command: &LinkCommand) {
        if let Ok(value) = serde_json::to_value(command) {
            self.commands.push(value);
        }
        if let (LinkOp::Rename, Some(name)) = (command.op, &command.name) {
            self.projection["deviceName"] = json!(name);
        }
        let _ = Storage::set_item(
            COMMANDS_KEY,
            Value::Array(self.commands.clone()).to_string(),
        );
    }
}
