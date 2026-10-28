//! 设置业务的 NSH 模块：设置持久化和异步名称确认留在 Rust，界面只转发端口命令。
// 设置引擎：纯逻辑，不依赖 nsh 运行时
mod engine;

// 引入 nsh 的本地元素、模块宏与存储
use nsh_core::{JsonValue, element::Local, prelude::*, storage::Storage};
// 引入内部可变性
use std::cell::RefCell;

#[derive(Default)]
// 随界面 VM 存活的设置运行时，包装引擎
struct SettingsRuntime(RefCell<engine::Engine>);

// VM 关闭时释放引擎
impl Local for SettingsRuntime {
    // 关闭
    fn close(&self) {
        // 销毁引擎
        self.0.borrow_mut().dispose();
    }
}

// 把 nsh 存储适配为引擎的存储接口
struct AppStorage;

// 应用存储实现引擎所需的存储接口
impl engine::Store for AppStorage {
    // 读取
    fn get(&self, key: &str) -> Option<String> {
        // 读取存储项
        Storage::get_item(key)
    }

    // 写入，错误转为文本
    fn set(&mut self, key: &str, value: String) -> Result<(), String> {
        // 写入存储项，错误转为文本
        Storage::set_item(key, value).map_err(|error| error.to_string())
    }
}

/// 设置用例。存储与原生确认共享 UI VM 的生命周期。
/// 每个方法对应引擎的一种请求,返回公开 UI 状态与待下发给 Link 端口的命令。
/// 方法是异步任务:核心任务提供 Storage 上下文,TS 的同步调用不会进入该上下文。
// 声明 nsh 原生模块
#[module]
// 设置用例模块
pub struct Settings;

// 把类型化的调用交给本 VM 的设置引擎，并返回公开状态
fn apply(request: engine::Request, sys: &System) -> Result<JsonValue, String> {
    // 取得（或创建）本 VM 的运行时
    let runtime = sys.locals.get::<SettingsRuntime>();
    // 应用请求
    let result = runtime.0.borrow_mut().apply(request, &mut AppStorage)?;
    // 序列化响应
    serde_json::to_value(&result).map_err(|error| error.to_string())
}

// 模块方法：函数名在 TS 中为 camelCase
#[module]
impl Settings {
    /// 启动:恢复设备身份和面板偏好,开启新会话。
    pub async fn boot(sys: System) -> Result<JsonValue, String> {
        apply(engine::Request::Boot, &sys)
    }

    /// 原生连接初始化完成,并带上首个公开视图。
    pub async fn initialized(session: u64, view: JsonValue, sys: System) -> Result<JsonValue, String> {
        apply(engine::Request::Initialized { session, view }, &sys)
    }

    /// 保存面板偏好。
    pub async fn save_panel(session: u64, panel: JsonValue, sys: System) -> Result<JsonValue, String> {
        apply(engine::Request::SavePanel { session, panel }, &sys)
    }

    /// 编辑设备名称草稿。
    pub async fn edit_name(session: u64, draft: String, sys: System) -> Result<JsonValue, String> {
        apply(engine::Request::EditName { session, draft }, &sys)
    }

    /// 开始保存设备名称,返回需要下发的命令。
    pub async fn begin_name_save(
        session: u64,
        draft: String,
        view: JsonValue,
        now_ms: u64,
        sys: System,
    ) -> Result<JsonValue, String> {
        apply(engine::Request::BeginNameSave { session, draft, view, now_ms }, &sys)
    }

    /// 观察原生视图,推进名称保存的确认与超时。
    pub async fn observe_name(
        session: u64,
        draft: String,
        view: JsonValue,
        now_ms: u64,
        sys: System,
    ) -> Result<JsonValue, String> {
        apply(engine::Request::ObserveName { session, draft, view, now_ms }, &sys)
    }

    /// 名称命令投递失败,回滚保存。
    pub async fn name_dispatch_failed(session: u64, sys: System) -> Result<JsonValue, String> {
        apply(engine::Request::NameDispatchFailed { session }, &sys)
    }

    /// 端口调用失败的上报。
    pub async fn port_failure(
        session: u64,
        view: JsonValue,
        stage: String,
        error: String,
        sys: System,
    ) -> Result<JsonValue, String> {
        apply(engine::Request::PortFailure { session, view, stage, error }, &sys)
    }

    /// 释放会话。
    pub async fn dispose(session: u64, sys: System) -> Result<JsonValue, String> {
        apply(engine::Request::Dispose { session }, &sys)
    }
}
