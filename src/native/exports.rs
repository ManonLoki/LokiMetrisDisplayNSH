//! NSH owns the connection on every target; no external application host is required.
use super::{bootstrap, connection, lifecycle};
use nsh_core::{element::Local, prelude::*};
use serde_json::Value;
use std::{cell::RefCell, sync::Arc};

struct Attached {
    handle: Arc<connection::Handle>,
    legacy_panel: Option<Value>,
    #[cfg(target_os = "ios")]
    _activity: lifecycle::ActivityObserver,
}

#[derive(Default)]
struct LinkRuntime(RefCell<Option<Attached>>);

impl Local for LinkRuntime {
    fn close(&self) {
        self.0.borrow_mut().take();
    }
}

fn with_handle<T>(sys: &System, f: impl FnOnce(&connection::Handle) -> T) -> Result<T, String> {
    let runtime = sys.locals.get::<LinkRuntime>();
    let attached = runtime.0.borrow();
    Ok(f(&attached.as_ref().ok_or("连接未初始化")?.handle))
}

fn view(sys: &System) -> Result<String, String> {
    let runtime = sys.locals.get::<LinkRuntime>();
    let attached = runtime.0.borrow();
    let attached = attached.as_ref().ok_or("连接未初始化")?;
    let raw = attached.handle.poll();
    let Some(panel) = &attached.legacy_panel else {
        return Ok(raw);
    };
    let mut value: Value = serde_json::from_str(&raw).map_err(|_| "连接投影无效")?;
    value["panelConfig"] = panel.clone();
    serde_json::to_string(&value).map_err(|_| "连接投影无效".into())
}

/// Authenticated LAN connection. Pairing secrets stay in Rust and the system credential store.
#[module]
pub struct Link;

#[module]
impl Link {
    /// Creates the VM's connection owner, restoring the existing device identity and private cache.
    pub fn initialize(device_id: String, name: String, sys: System) -> Result<String, String> {
        let runtime = sys.locals.get::<LinkRuntime>();
        if runtime.0.borrow().is_none() {
            let root = sys.data_dir.clone().ok_or("应用私有目录不可用")?;
            #[cfg(debug_assertions)]
            let root = match std::env::var_os("LOKI_TEST_DATA_DIR") {
                Some(path) => {
                    let path = std::path::PathBuf::from(path);
                    if !path.is_absolute() {
                        return Err("测试目录必须是绝对路径".into());
                    }
                    path
                }
                None => root,
            };
            let config = bootstrap::configure(&root, &device_id, &name)?;
            let legacy_panel = std::fs::read(config.root.join("panel-v1.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .or_else(|| config.legacy_panel.clone());
            let handle = Arc::new(connection::Handle::new(config)?);
            #[cfg(target_os = "ios")]
            let activity = bootstrap::observe(Arc::downgrade(&handle));
            #[cfg(target_os = "android")]
            bootstrap::observe(Arc::downgrade(&handle));
            *runtime.0.borrow_mut() = Some(Attached {
                handle,
                legacy_panel,
                #[cfg(target_os = "ios")]
                _activity: activity,
            });
        }
        view(&sys)
    }

    /// Enqueues discovery, selection, forgetting, renaming or refresh in the same owner.
    pub fn command(json: String, sys: System) -> Result<(), String> {
        with_handle(&sys, |handle| handle.command(&json))?
    }

    /// Returns the public connection state and verified image URLs.
    pub fn poll(sys: System) -> Result<String, String> {
        view(&sys)
    }
}

/// Read-only NSH application activity and input form facts.
#[module]
pub struct UiLifecycle;

#[module]
impl UiLifecycle {
    /// Returns unknown when the current platform cannot establish foreground visibility.
    pub fn foreground(sys: System) -> Option<bool> {
        lifecycle::foreground(sys.os.name())
    }

    /// Identifies television input without exposing retired system-extension capabilities.
    pub fn form_factor() -> String {
        bootstrap::form_factor()
    }
}
