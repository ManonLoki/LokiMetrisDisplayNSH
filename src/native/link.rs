//! NSH 在每个目标上都持有连接;不需要外部应用宿主。
use super::{bootstrap, connection};
use crate::features::connection::view_state;
use crate::shared::contracts::models::{LinkCommand, LinkOp, ViewState, typed};
use nsh_core::{JsonValue, element::Local, prelude::*};
use std::{cell::RefCell, sync::Arc};

struct Attached {
    // 连接 owner 的句柄
    handle: Arc<connection::Handle>,
    // 随界面可见性变化的系统资源，随连接一起释放
    visibility: bootstrap::Visibility,
    // 上一次交给 UI 的投影文本，用于只在变化时返回新视图
    last_view: Option<String>,
}

#[derive(Default)]
// 随 VM 存活的连接运行时
struct LinkRuntime(RefCell<Option<Attached>>);

// VM 关闭时释放连接
impl Local for LinkRuntime {
    fn close(&self) {
        self.0.borrow_mut().take();
    }
}

// 在已初始化的连接上执行操作，未初始化则报错
fn with_handle<T>(sys: &System, f: impl FnOnce(&connection::Handle) -> T) -> Result<T, String> {
    let runtime = sys.locals.get::<LinkRuntime>();
    let attached = runtime.0.borrow();
    Ok(f(&attached.as_ref().ok_or("连接未初始化")?.handle))
}

/// 首个视图：校验后记录，之后的轮询只在变化时返回。
fn initial_view(sys: &System) -> Result<JsonValue, String> {
    let runtime = sys.locals.get::<LinkRuntime>();
    let mut attached = runtime.0.borrow_mut();
    let attached = attached.as_mut().ok_or("连接未初始化")?;
    let raw = attached.handle.poll();
    // 校验并转换为公开视图
    let view = view_state::native_view(&raw)?;
    // 记录，之后的轮询只在变化时返回
    attached.last_view = Some(raw);
    Ok(view)
}

/// 已认证的局域网连接。配对机密留在 Rust 和系统凭据存储中。
// 经认证的局域网连接；配对秘密只留在 Rust 与系统凭据库
pub(crate) struct Link;

// 真实连接的操作；Shell 在产品里调用它们
impl Link {
    /// 创建 VM 的连接所有者,恢复现有的设备身份和私有缓存。
    // 创建本 VM 的连接 owner，恢复设备身份与私有缓存
    pub(crate) fn initialize(
        device_id: &str,
        name: &str,
        idiom: &str,
        sys: &System,
    ) -> Result<ViewState, String> {
        let runtime = sys.locals.get::<LinkRuntime>();
        // 只初始化一次
        if runtime.0.borrow().is_none() {
            let root = sys.data_dir.clone().ok_or("应用私有目录不可用")?;
            // 调试构建允许用环境变量指定测试目录
            #[cfg(debug_assertions)]
            let root = match std::env::var_os("LOKI_TEST_DATA_DIR") {
                Some(path) => {
                    let path = std::path::PathBuf::from(path);
                    // 必须是绝对路径
                    if !path.is_absolute() {
                        // 必须是绝对路径
                        return Err("测试目录必须是绝对路径".into());
                    }
                    path
                }
                // 否则用默认根目录
                None => root,
            };
            // 生成各平台的连接配置
            let mut config = bootstrap::configure(&root, device_id, name)?;
            if config.platform == "android" || config.platform == "ios" {
                // NSH 将平板称为 pad，连接协议使用 tablet。只上报手机和平板。
                config.form_factor = match idiom {
                    "phone" => Some("phone".into()),
                    "pad" => Some("tablet".into()),
                    _ => None,
                };
            }
            // 创建连接 owner
            let handle = Arc::new(connection::Handle::new(config)?);
            *runtime.0.borrow_mut() = Some(Attached {
                visibility: bootstrap::Visibility::default(),
                handle,
                // 首次视图在下面的 initial_view 中记录
                last_view: None,
            });
        }
        typed(initial_view(sys)?)
    }

    /// 在同一个所有者中,为发现、选择、遗忘、重命名或刷新入队。
    // 把发现、选择、忘记、重命名或刷新命令排入 owner 队列
    pub(crate) fn command(command: &LinkCommand, sys: &System) -> Result<(), String> {
        // 序列化为连接所有者读取的文本
        let text = serde_json::to_string(command).map_err(|_| "无效连接操作")?;
        // 界面可见性同时驱动平台资源与 Watch 缓存的重试
        if let (LinkOp::Active, Some(visible)) = (command.op, command.value) {
            let runtime = sys.locals.get::<LinkRuntime>();
            if let Some(attached) = runtime.0.borrow().as_ref() {
                attached.visibility.set(visible);
            }
            if visible {
                super::companion::foregrounded();
            }
        }
        with_handle(sys, |handle| handle.command(&text))?
    }

    pub(crate) async fn wait_for_view_change(sys: &System) -> bool {
        let runtime = sys.locals.get::<LinkRuntime>();
        let (publications, previous) = {
            let attached = runtime.0.borrow();
            let Some(attached) = attached.as_ref() else {
                return false;
            };
            (attached.handle.publications(), attached.last_view.clone())
        };
        publications
            .wait_for_change(previous.as_deref().unwrap_or_default())
            .await
    }

    pub(crate) fn close(sys: &System) {
        sys.locals.get::<LinkRuntime>().close();
    }

    /// 返回已验证的公开连接视图;自上次返回以来没有变化时返回 null。
    // 只在投影变化时校验并返回新视图
    pub(crate) fn poll(sys: &System) -> Result<Option<ViewState>, String> {
        let runtime = sys.locals.get::<LinkRuntime>();
        let mut attached = runtime.0.borrow_mut();
        let attached = attached.as_mut().ok_or("连接未初始化")?;
        let raw = attached.handle.poll();
        if attached.last_view.as_deref() == Some(raw.as_str()) {
            return Ok(None);
        }
        // 记录已读取的发布，解析失败时也等待下一次变化。
        let raw = attached.last_view.insert(raw);
        typed(view_state::native_view(raw)?).map(Some)
    }
}
