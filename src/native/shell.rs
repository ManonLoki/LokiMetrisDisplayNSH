//! 应用外壳控制器:启动、轮询、保存名称/面板和命令转发都在 Rust 中完成。
//! TSX 只持有 UI 时钟和信号,把每次调用返回的公开状态原样赋给信号。
// 引入设置用例与公开数据形状
use super::Link;
use crate::features::settings::Settings;
use crate::shared::contracts::models::{LinkCommand, PanelConfig, SettingsResult, ViewState};
// 引入 nsh 本地元素与模块宏
use nsh_core::{element::Local, prelude::*};
// 引入内部可变性
#[cfg(debug_assertions)]
use super::fixture::Fixture;
use std::cell::RefCell;
#[cfg(debug_assertions)]
use std::rc::Rc;

// 连接后端：产品用真实连接；调试构建里测试可用 Rust 夹具扮演桌面端
#[derive(Clone, Default)]
enum Backend {
    // 真实连接 owner
    #[default]
    Real,
    // 测试夹具（仅调试构建）
    #[cfg(debug_assertions)]
    Fixture(Rc<RefCell<Fixture>>),
}

// 外壳状态：后端、最近一次公开视图与阶段标记
#[derive(Default)]
struct ShellState {
    // 连接后端
    backend: Backend,
    // 最近一次交给 UI 的视图，名称确认和失败上报都要用
    view: Option<ViewState>,
    // 当前设置会话
    session: u64,
    // 连接端口是否已初始化，之后才允许下发命令
    started: bool,
    // 初始化流程是否完成，之后才轮询
    ready: bool,
    // 已释放，之后的调用都不再产生变化
    disposed: bool,
}

// 随 VM 存活的外壳运行时
#[derive(Default)]
struct ShellRuntime(RefCell<ShellState>);

// 无需额外清理：设置引擎和连接各自随 VM 释放
impl Local for ShellRuntime {
    fn close(&self) {}
}

// 取外壳状态的副本字段
fn with<T>(sys: &System, f: impl FnOnce(&mut ShellState) -> T) -> T {
    // 取运行时
    let runtime = sys.locals.get::<ShellRuntime>();
    // 借用并执行
    let mut state = runtime.0.borrow_mut();
    f(&mut state)
}

// 合并连续两次设置结果：名称相关字段以后者为准，可选字段后者缺省时沿用前者
fn merge(base: SettingsResult, next: SettingsResult) -> SettingsResult {
    SettingsResult {
        session: next.session,
        saving_name: next.saving_name,
        name_issue: next.name_issue,
        name_feedback: next.name_feedback,
        panel_issue: next.panel_issue,
        panel: next.panel.or(base.panel),
        device: next.device.or(base.device),
        initialize: next.initialize.or(base.initialize),
        draft: next.draft.or(base.draft),
        view: next.view.or(base.view),
        // 命令由外壳自己下发，不再交给 TS
        commands: Vec::new(),
    }
}

// 记录结果里的会话与视图
fn remember(sys: &System, result: &SettingsResult) {
    with(sys, |state| {
        state.session = result.session;
        if let Some(view) = &result.view {
            state.view = Some(view.clone());
        }
    });
}

// 取最近的视图，尚无时报错
fn current_view(sys: &System) -> Result<ViewState, String> {
    with(sys, |state| state.view.clone()).ok_or_else(|| "连接视图尚未建立".to_owned())
}

// 初始化连接
#[cfg_attr(not(debug_assertions), allow(unused_variables))]
fn link_initialize(
    sys: &System,
    backend: &Backend,
    id: &str,
    name: &str,
    now_ms: u64,
) -> Result<ViewState, String> {
    match backend {
        // 真实连接
        Backend::Real => Link::initialize(id, name, sys),
        // 夹具
        #[cfg(debug_assertions)]
        Backend::Fixture(fixture) => fixture.borrow_mut().initialize(id, name, now_ms),
    }
}

// 轮询连接，没有变化时为 None
#[cfg_attr(not(debug_assertions), allow(unused_variables))]
fn link_poll(sys: &System, backend: &Backend, now_ms: u64) -> Result<Option<ViewState>, String> {
    match backend {
        // 真实连接
        Backend::Real => Link::poll(sys),
        // 夹具
        #[cfg(debug_assertions)]
        Backend::Fixture(fixture) => fixture.borrow_mut().poll(now_ms),
    }
}

// 下发命令
#[cfg_attr(not(debug_assertions), allow(unused_variables))]
fn link_command(
    sys: &System,
    backend: &Backend,
    command: &LinkCommand,
    now_ms: u64,
) -> Result<(), String> {
    match backend {
        // 真实连接
        Backend::Real => Link::command(command, sys),
        // 夹具
        #[cfg(debug_assertions)]
        Backend::Fixture(fixture) => {
            fixture.borrow_mut().command(command, now_ms);
            Ok(())
        }
    }
}

// 上报端口失败，让设置引擎回滚；上报本身失败时只返回该错误
fn fail(sys: &System, stage: &str, error: String) -> Result<SettingsResult, String> {
    let view = current_view(sys)?;
    let session = with(sys, |state| state.session);
    let result = Settings::port_failure(session, &view, stage.to_owned(), error, sys)?;
    remember(sys, &result);
    Ok(result)
}

// 轮询一次并让设置引擎观察名称确认的时钟
async fn poll_and_observe(
    sys: &System,
    draft: String,
    now_ms: u64,
) -> Result<SettingsResult, String> {
    // 取后端
    let backend = with(sys, |state| state.backend.clone());
    // 轮询：只有变化时才有新视图
    let changed = match link_poll(sys, &backend, now_ms) {
        Ok(changed) => changed,
        Err(error) => return fail(sys, "poll", error),
    };
    // 记下新视图
    if let Some(view) = &changed {
        with(sys, |state| state.view = Some(view.clone()));
    }
    // 即使连接投影未变，也让 Rust 观察时钟，名称确认才能按时结束
    let view = current_view(sys)?;
    let session = with(sys, |state| state.session);
    let mut result = Settings::observe_name(session, draft, &view, now_ms, sys)?;
    remember(sys, &result);
    // 视图变化要通知 UI
    if changed.is_some() {
        result.view = changed;
    }
    Ok(result)
}

// 下发一条命令，随后立即轮询
async fn command_then_poll(
    sys: &System,
    command: &LinkCommand,
    draft: String,
    now_ms: u64,
) -> Result<Option<SettingsResult>, String> {
    // 取后端
    let backend = with(sys, |state| state.backend.clone());
    // 端口未启动或已销毁时忽略
    if !with(sys, |state| state.started && !state.disposed) {
        return Ok(None);
    }
    // 下发
    if let Err(error) = link_command(sys, &backend, command, now_ms) {
        return fail(sys, "command", error).map(Some);
    }
    // 随后轮询
    poll_and_observe(sys, draft, now_ms).await.map(Some)
}

/// 应用外壳:TSX 调用它们并把返回的公开状态赋给信号。
// 外壳模块
#[module]
pub struct Shell;

// 模块方法
#[module]
impl Shell {
    /// 启动:恢复身份和偏好,初始化连接,返回首个公开状态。
    /// 调试构建里,存储中有测试夹具键时由 Rust 夹具扮演桌面端,否则使用真实连接。
    pub async fn start(now_ms: u64, sys: System) -> Result<SettingsResult, String> {
        // 选择后端
        #[cfg(debug_assertions)]
        let backend = Fixture::select().map_or(Backend::Real, |fixture| {
            Backend::Fixture(Rc::new(RefCell::new(fixture)))
        });
        #[cfg(not(debug_assertions))]
        let backend = Backend::Real;
        with(&sys, |state| state.backend = backend.clone());
        // 恢复身份和偏好
        let restored = Settings::boot(&sys)?;
        remember(&sys, &restored);
        let identity = restored
            .initialize
            .clone()
            .ok_or_else(|| "设置未提供设备身份".to_owned())?;
        // 初始化连接
        let view = match link_initialize(&sys, &backend, &identity.id, &identity.name, now_ms) {
            Ok(view) => view,
            Err(error) => {
                // 失败：用空视图占位并上报
                let device = restored
                    .device
                    .clone()
                    .ok_or_else(|| "设置未提供设备身份".to_owned())?;
                let empty = crate::shared::contracts::models::typed::<ViewState>(
                    crate::features::connection::view_state::empty_view(&device.id, &device.name),
                )?;
                with(&sys, |state| state.view = Some(empty.clone()));
                let failed = fail(&sys, "initialize", error)?;
                let mut merged = merge(restored, failed);
                merged.view = merged.view.or(Some(empty));
                return Ok(merged);
            }
        };
        // 端口已启动
        with(&sys, |state| {
            state.started = true;
            state.view = Some(view.clone());
        });
        // 通知设置引擎初始化完成
        let session = with(&sys, |state| state.session);
        let initialized = Settings::initialized(session, &view, &sys)?;
        remember(&sys, &initialized);
        with(&sys, |state| state.ready = true);
        let mut merged = merge(restored, initialized);
        merged.view = Some(view);
        Ok(merged)
    }

    /// 每个 UI 时钟周期调用:轮询连接并推进名称确认。初始化完成前不做任何事。
    pub async fn tick(
        draft: String,
        now_ms: u64,
        sys: System,
    ) -> Result<Option<SettingsResult>, String> {
        // 未就绪或已释放：无变化
        if !with(&sys, |state| state.ready && !state.disposed) {
            return Ok(None);
        }
        poll_and_observe(&sys, draft, now_ms).await.map(Some)
    }

    /// 下发一条连接命令(选择、遗忘、刷新等)并立即轮询。
    pub async fn command(
        command: LinkCommand,
        draft: String,
        now_ms: u64,
        sys: System,
    ) -> Result<Option<SettingsResult>, String> {
        if with(&sys, |state| state.disposed) {
            return Ok(None);
        }
        command_then_poll(&sys, &command, draft, now_ms).await
    }

    /// 编辑设备名称草稿。
    pub async fn edit_name(draft: String, sys: System) -> Result<SettingsResult, String> {
        let session = with(&sys, |state| state.session);
        match Settings::edit_name(session, draft, &sys) {
            Ok(result) => {
                remember(&sys, &result);
                Ok(result)
            }
            Err(error) => fail(&sys, "settings", error),
        }
    }

    /// 保存设备名称:设置引擎给出命令,外壳下发并轮询。
    pub async fn save_name(
        draft: String,
        now_ms: u64,
        sys: System,
    ) -> Result<Option<SettingsResult>, String> {
        if with(&sys, |state| state.disposed) {
            return Ok(None);
        }
        let backend = with(&sys, |state| state.backend.clone());
        let view = current_view(&sys)?;
        let session = with(&sys, |state| state.session);
        let begun = Settings::begin_name_save(session, draft.clone(), &view, now_ms, &sys)?;
        remember(&sys, &begun);
        // 逐条下发命令；任何一条失败就回滚本次保存
        for command in &begun.commands {
            if link_command(&sys, &backend, command, now_ms).is_err() {
                let session = with(&sys, |state| state.session);
                let rolled = Settings::name_dispatch_failed(session, &sys)?;
                remember(&sys, &rolled);
                return Ok(Some(merge(begun, rolled)));
            }
        }
        // 有命令才需要立即轮询
        if begun.commands.is_empty() {
            return Ok(Some(merge(begun.clone(), begun)));
        }
        let polled = poll_and_observe(&sys, draft, now_ms).await?;
        Ok(Some(merge(begun, polled)))
    }

    /// 保存面板偏好:设置引擎落盘,外壳下发它给出的命令。
    pub async fn save_panel(
        panel: PanelConfig,
        draft: String,
        now_ms: u64,
        sys: System,
    ) -> Result<Option<SettingsResult>, String> {
        if with(&sys, |state| state.disposed) {
            return Ok(None);
        }
        let session = with(&sys, |state| state.session);
        let saved = Settings::save_panel(session, &panel, &sys)?;
        remember(&sys, &saved);
        let mut merged = SettingsResult {
            commands: Vec::new(),
            ..saved.clone()
        };
        for command in &saved.commands {
            if let Some(next) = command_then_poll(&sys, command, draft.clone(), now_ms).await? {
                merged = merge(merged, next);
            }
        }
        Ok(Some(merged))
    }

    /// 释放:结束设置会话,并告诉连接界面不再活动。
    pub async fn dispose(sys: System) -> Result<(), String> {
        if with(&sys, |state| std::mem::replace(&mut state.disposed, true)) {
            return Ok(());
        }
        let backend = with(&sys, |state| state.backend.clone());
        let session = with(&sys, |state| state.session);
        // VM 可能已开始关闭，失败不再上报
        let _ = Settings::dispose(session, &sys);
        if with(&sys, |state| state.started) {
            let inactive = LinkCommand {
                op: crate::shared::contracts::models::LinkOp::Active,
                client_id: None,
                name: None,
                value: Some(false),
            };
            let _ = link_command(&sys, &backend, &inactive, 0);
        }
        Ok(())
    }
}
