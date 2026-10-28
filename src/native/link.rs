//! NSH 在每个目标上都持有连接;不需要外部应用宿主。
// 引入启动配置与连接
use super::{bootstrap, connection};
// 引入 nsh 本地元素与模块宏
use nsh_core::{element::Local, prelude::*};
// 引入 JSON 值
use serde_json::Value;
// 引入内部可变性与共享指针
use std::{cell::RefCell, sync::Arc};

// 已建立的连接
struct Attached {
    // 连接 owner 的句柄
    handle: Arc<connection::Handle>,
    // 旧版移动端遗留的面板配置，仅在首次视图携带一次
    legacy_panel: Option<Value>,
    // 前后台观察守卫，随连接一起释放
    _activity: bootstrap::ActivityGuard,
}

#[derive(Default)]
// 随 VM 存活的连接运行时
struct LinkRuntime(RefCell<Option<Attached>>);

// VM 关闭时释放连接
impl Local for LinkRuntime {
    // 关闭连接
    fn close(&self) {
        // 取走并丢弃句柄
        self.0.borrow_mut().take();
    }
}

// 在已初始化的连接上执行操作，未初始化则报错
fn with_handle<T>(sys: &System, f: impl FnOnce(&connection::Handle) -> T) -> Result<T, String> {
    // 取连接运行时
    let runtime = sys.locals.get::<LinkRuntime>();
    // 借用已挂载的句柄
    let attached = runtime.0.borrow();
    // 未初始化则报错
    Ok(f(&attached.as_ref().ok_or("连接未初始化")?.handle))
}

/// 第一个视图还携带移动宿主的旧版面板;设置页会在 `initialized` 时
/// 采纳它一次,因此常规轮询可以省去额外的 JSON 往返。
// 首次视图附带旧面板配置，Settings 在 initialized 时采纳一次，之后的轮询不再多一次 JSON 往返
fn initial_view(sys: &System) -> Result<String, String> {
    // 取连接运行时
    let runtime = sys.locals.get::<LinkRuntime>();
    // 借用已挂载的句柄
    let attached = runtime.0.borrow();
    // 未初始化则报错
    let attached = attached.as_ref().ok_or("连接未初始化")?;
    // 取当前投影
    let raw = attached.handle.poll();
    // 没有旧配置则直接返回
    let Some(panel) = &attached.legacy_panel else {
        // 返回原始文本
        return Ok(raw);
    };
    // 否则把旧配置写进投影
    let mut value: Value = serde_json::from_str(&raw).map_err(|_| "连接投影无效")?;
    // 附上面板配置
    value["panelConfig"] = panel.clone();
    // 序列化，失败报连接投影无效
    serde_json::to_string(&value).map_err(|_| "连接投影无效".into())
}

/// 已认证的局域网连接。配对机密留在 Rust 和系统凭据存储中。
// 声明 nsh 原生模块
#[module]
// 经认证的局域网连接；配对秘密只留在 Rust 与系统凭据库
pub struct Link;

// 模块方法
#[module]
impl Link {
    /// 创建 VM 的连接所有者,恢复现有的设备身份和私有缓存。
    // 创建本 VM 的连接 owner，恢复设备身份与私有缓存
    pub fn initialize(device_id: String, name: String, sys: System) -> Result<String, String> {
        // 取连接运行时
        let runtime = sys.locals.get::<LinkRuntime>();
        // 只初始化一次
        if runtime.0.borrow().is_none() {
            // 应用私有目录
            let root = sys.data_dir.clone().ok_or("应用私有目录不可用")?;
            // 调试构建允许用环境变量指定测试目录
            #[cfg(debug_assertions)]
            let root = match std::env::var_os("LOKI_TEST_DATA_DIR") {
                // 指定了测试目录
                Some(path) => {
                    // 转为路径
                    let path = std::path::PathBuf::from(path);
                    // 必须是绝对路径
                    if !path.is_absolute() {
                        // 必须是绝对路径
                        return Err("测试目录必须是绝对路径".into());
                    }
                    // 使用该路径
                    path
                }
                // 否则用默认根目录
                None => root,
            };
            // 生成各平台的连接配置
            let config = bootstrap::configure(&root, &device_id, &name)?;
            // 读取旧版面板文件，没有则用配置里带的
            let legacy_panel = std::fs::read(config.root.join("panel-v1.json"))
                // 读取成功
                .ok()
                // 解析为 JSON
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                // 否则使用配置里的旧版面板
                .or_else(|| config.legacy_panel.clone());
            // 创建连接 owner
            let handle = Arc::new(connection::Handle::new(config)?);
            // 保存连接，并开始观察系统前后台
            *runtime.0.borrow_mut() = Some(Attached {
                // 观察前后台变化
                _activity: bootstrap::observe(Arc::downgrade(&handle)),
                // 句柄
                handle,
                // 旧版面板
                legacy_panel,
            });
        }
        // 返回首次视图
        initial_view(&sys)
    }

    /// 在同一个所有者中,为发现、选择、遗忘、重命名或刷新入队。
    // 把发现、选择、忘记、重命名或刷新命令排入 owner 队列
    pub fn command(json: String, sys: System) -> Result<(), String> {
        // 把命令交给句柄
        with_handle(&sys, |handle| handle.command(&json))?
    }

    /// 返回公开的连接状态和已验证的图片 URL。
    // 返回公开连接状态与已校验的图片地址
    pub fn poll(sys: System) -> Result<String, String> {
        // 轮询句柄状态
        with_handle(&sys, |handle| handle.poll())
    }
}
