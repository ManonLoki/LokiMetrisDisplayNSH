//! 设置业务的 NSH 模块：设置持久化和异步名称确认留在 Rust，界面只转发端口命令。
// 设置引擎：纯逻辑，不依赖 nsh 运行时
mod engine;

// 引入 nsh 的本地元素、模块宏与存储
use nsh_core::{element::Local, prelude::*, storage::Storage};
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
// 声明 nsh 原生模块
#[module]
// 设置用例模块
pub struct Settings;

// 模块方法
#[module]
impl Settings {
    /// 返回公开的 UI 状态,以及供注入的 Link 端口分发的命令。
    /// 核心任务提供 Storage 上下文,TS 的同步调用不会进入该上下文。
    // 唯一入口：解析请求、交给引擎，返回公开状态与待下发命令；异步任务才能进入存储上下文
    pub async fn call(request: String, sys: System) -> Result<String, String> {
        // 解析请求
        let request =
            // 解析请求，失败报请求无效
            serde_json::from_str(&request).map_err(|error| format!("设置请求无效: {error}"))?;
        // 取得（或创建）本 VM 的运行时
        let runtime = sys.locals.get::<SettingsRuntime>();
        // 应用请求
        let result = runtime.0.borrow_mut().apply(request, &mut AppStorage)?;
        // 序列化响应
        serde_json::to_string(&result).map_err(|error| error.to_string())
    }
}
