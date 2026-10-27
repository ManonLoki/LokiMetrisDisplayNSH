//! 设置业务的 NSH 模块：设置持久化和异步名称确认留在 Rust，界面只转发端口命令。
mod engine;

use nsh_core::{element::Local, prelude::*, storage::Storage};
use std::cell::RefCell;

#[derive(Default)]
struct SettingsRuntime(RefCell<engine::Engine>);

impl Local for SettingsRuntime {
    fn close(&self) {
        self.0.borrow_mut().dispose();
    }
}

struct AppStorage;

impl engine::Store for AppStorage {
    fn get(&self, key: &str) -> Option<String> {
        Storage::get_item(key)
    }

    fn set(&mut self, key: &str, value: String) -> Result<(), String> {
        Storage::set_item(key, value).map_err(|error| error.to_string())
    }
}

/// Settings use cases. Storage and native confirmation share the UI VM's lifetime.
#[module]
pub struct Settings;

#[module]
impl Settings {
    /// Returns public UI state and commands for the injected Link port to dispatch.
    /// Core tasks supply the Storage context that synchronous calls from TS do not enter.
    pub async fn call(request: String, sys: System) -> Result<String, String> {
        let request =
            serde_json::from_str(&request).map_err(|error| format!("设置请求无效: {error}"))?;
        let runtime = sys.locals.get::<SettingsRuntime>();
        let result = runtime.0.borrow_mut().apply(request, &mut AppStorage)?;
        serde_json::to_string(&result).map_err(|error| error.to_string())
    }
}
