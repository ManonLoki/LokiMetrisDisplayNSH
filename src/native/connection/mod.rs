//! 展示端连接入口和单线程 owner，桌面与移动端共用。
//! NSH 提交有限的命令并读取公开状态;网络与存储归一个所有者线程负责。
// 前后台与网络变化对 owner 的影响
mod app_visibility;
// 图片缓存校验
mod asset_cache;
// 图片资源下载与落盘
mod assets;
// 界面命令的入队与处理
mod commands;
// 局域网桌面发现
mod discovery;
// 带分帧的套接字
mod framed_socket;
// 单线程连接 owner
mod owner;
// 配对协议
mod pairing;
// 本地持久化
mod persistence;
// 公开视图投影
mod public_view;
// 认证后的会话
mod session;
// 快照准入校验
mod snapshot_admission;
// 小组件共享
mod widget;

// 仅 macOS 的配套设备测试
#[cfg(all(test, target_os = "macos"))]
mod companion_test;

// 引入命令类型
use commands::{CommandIntent, QueuedCommand};
// 引入设备 ID 与名称校验
use loki_metris_pairing::{valid_device_id, valid_device_name};
// 引入序列化
use serde::{Deserialize, Serialize};
// 引入路径、同步原语、线程与时间
use std::{
    // 路径类型
    path::PathBuf,
    // 同步原语
    sync::{
        // 原子引用计数与互斥锁
        Arc,
        // 互斥锁
        Mutex,
        // 原子类型与内存序
        atomic::{AtomicBool, AtomicU64, Ordering},
        // 通道
        mpsc::{self, SyncSender},
    },
    // 线程
    thread,
    // 时间与纪元
    time::{SystemTime, UNIX_EPOCH},
};

// 桌面广播的服务类型
const SERVICE: &str = "_loki-host._tcp.local.";
// 系统凭据库中的服务名
const CREDENTIAL_SERVICE: &str = "com.manonloki.lokimetris.display.pairing.v2";
// 单帧最大字节数
const MAX_FRAME: usize = 96 * 1024;
// 握手阶段单帧上限
const HANDSHAKE_FRAME: usize = 4096;
// 当前 Unix 毫秒时间
fn now_ms() -> u64 {
    // 当前系统时间
    SystemTime::now()
        // 距纪元的时长
        .duration_since(UNIX_EPOCH)
        // 出错取 0
        .unwrap_or_default()
        // 转为毫秒数
        .as_millis() as u64
}
// 把任意错误统一为面向用户的文案，不泄露细节
fn err(_: impl std::fmt::Display) -> String {
    // 统一的本地失败提示
    "本地连接或存储操作失败，请重试。".into()
}

/// 界面可提交的连接操作；宿主专用操作另见 [`HostCommand`]。
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
// 界面可提交的连接操作；宿主专用操作另见 HostCommand
enum Command {
    // 重新发现
    Discover,
    // 选择桌面
    Select {
        #[serde(rename = "clientId")]
        client_id: String,
    },
    // 忘记桌面
    Forget {
        #[serde(rename = "clientId")]
        client_id: String,
    },
    // 修改本机名称
    Rename {
        // 设备名
        name: String,
    },
    /// 桌面端可见性;移动端的活动状态由 NSH 系统适配器提供。
    // 桌面端可见性
    Active {
        // 开关值
        value: bool,
    },
    // 刷新
    Refresh,
    // 宿主命令只能由原生侧构造，JS 无法提交
    #[serde(skip)]
    #[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
    Host(HostCommand),
    #[serde(skip)]
    // 停止 owner
    Stop,
}

/// 来自 NSH 运行时适配器的系统事实,JavaScript 命令无法获得。
#[derive(Deserialize)]
#[serde(
    // 用 op 字段区分命令
    tag = "op",
    // 变体名用驼峰
    rename_all = "camelCase",
    // 变体字段名用驼峰
    rename_all_fields = "camelCase",
    // 拒绝未知字段
    deny_unknown_fields
)]
// 来自 NSH 运行时适配层的系统事实，JS 不能提交
pub(crate) enum HostCommand {
    /// 应用可见性与本地网络权限。
    // 应用可见性与本地网络权限
    Lifecycle { visible: bool, permitted: bool },
    /// 系统网络路径变化后重新发现。
    // 系统网络路径变化后重新发现
    Network,
}

/// NSH 运行时配置,在移动目标上使用系统身份。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
// 连接配置，移动端带系统身份
pub(crate) struct Config {
    /// 应用私有目录；owner 在其下建立 `loki-link-v1`。
    // 应用私有目录；owner 在其下建立 loki-link-v1
    pub root: PathBuf,
    #[serde(default)]
    // 设备 ID
    pub device_id: String,
    #[serde(default)]
    // 设备名称
    pub name: String,
    /// 为 true 时,由系统凭据存储持有移动端身份。
    #[serde(default)]
    // 为 true 时由系统凭据库持有移动端身份
    pub host_identity: bool,
    /// 由 NSH 系统适配器提供活动状态;UI 的 active 命令会被忽略。
    #[serde(default)]
    // 为 true 时前后台由系统适配层提供，界面的 active 命令被忽略
    pub host_lifecycle: bool,
    #[serde(default = "default_platform")]
    // 平台名
    pub platform: String,
    /// 可信状态提交时为配对的 Apple Watch 发布公开缓存。
    #[serde(default)]
    // 是否为配对的 Apple Watch 发布公开缓存
    pub watch_companion: bool,
    /// 系统授予的 WidgetKit App Group;绝不由 JavaScript 提供。
    #[serde(default)]
    // 系统授予的小组件 App Group 目录，绝不由 JS 提供
    pub widget_dir: Option<PathBuf>,
    #[serde(default = "default_version")]
    // 应用版本
    pub app_version: String,
    /// `phone` 或 `tablet`；桌面不上报形态。
    #[serde(default)]
    // phone 或 tablet；桌面不上报
    pub form_factor: Option<String>,
    /// 离线时是否保留并恢复最近快照。
    #[serde(default = "yes")]
    // 离线时是否保留并恢复最近快照
    pub keep_snapshot: bool,
    /// 原生宿主旧存储中的面板偏好，只在 Rust 尚无保存值时导入一次。
    #[serde(default)]
    #[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
    // 旧版原生宿主存储的面板偏好，Rust 尚无保存值时只导入一次
    pub legacy_panel: Option<serde_json::Value>,
}
// 默认取编译目标平台
fn default_platform() -> String {
    // 平台名取自编译目标
    std::env::consts::OS.into()
}
// 默认版本号
fn default_version() -> String {
    // 默认应用版本
    "1.0.0".into()
}
// 默认值 true
fn yes() -> bool {
    // 默认保留快照
    true
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
// 配对材料，保存到系统凭据库
struct Material {
    // 客户端标识
    client_id: String,
    // 设备标识
    device_id: String,
    // 设备名
    name: String,
    // 配对秘密
    secret: String,
    // 保存时间
    saved_at_ms: u64,
}

/// 发现得到的端点；`key` 为服务实例全名，同一桌面多个地址时以此区分。
#[derive(Clone)]
// 发现得到的端点：身份、名称与地址
struct Endpoint {
    // 标识
    id: String,
    // 设备名
    name: String,
    // 地址
    address: std::net::SocketAddr,
}

/// owner 的唯一事件入口：命令、连接线程与发现线程共享同一个队列。
// owner 的唯一事件入口：命令、连接线程与发现线程共用同一队列
enum Event {
    // 命令事件
    Command(QueuedCommand),
    // 套接字事件
    Socket(u64, framed_socket::SocketEvent),
    // 发现事件
    Discovery(discovery::Found),
}

/// NSH 应用读取的最新公开投影。
#[derive(Default)]
// 最近一次公开投影，供 NSH 应用读取
struct Published {
    // 已发布状态（加锁）
    state: Mutex<PublishedState>,
}
#[derive(Default, Clone)]
struct PublishedState {
    // 视图 JSON 文本
    view: String,
}

// 连接句柄：向 owner 投递命令并读取公开状态
pub(crate) struct Handle {
    // 事件队列发送端
    tx: SyncSender<Event>,
    // 取消代数
    revision: Arc<AtomicU64>,
    // 已入队的选择意图
    intent: Mutex<CommandIntent>,
    // 公开投影
    published: Arc<Published>,
    // 宿主生命周期
    host_lifecycle: bool,
    // 是否已停止
    stopped: Arc<AtomicBool>,
}
// 句柄的方法
impl Handle {
    // 创建句柄并启动 owner 线程
    pub fn new(config: Config) -> Result<Self, String> {
        // 建立私有目录
        let root = config.root.join("loki-link-v1");
        // 确保根目录为私有目录
        persistence::private_dir(&root)?;
        // 读取或生成设备身份并校验
        let (id, name) = persistence::identity(&root, &config)?;
        // 设备标识或名称不合法
        if !valid_device_id(&id) || !valid_device_name(&name) {
            // 设备身份无效
            return Err("设备身份无效".into());
        }
        // 已发布状态
        let published = Arc::new(Published::default());
        // 容量 256 的有界队列
        let (tx, rx) = mpsc::sync_channel(256);
        // 修订号
        let revision = Arc::new(AtomicU64::new(0));
        // 是否由宿主管理生命周期
        let host_lifecycle = config.host_lifecycle;
        // 停止标记
        let stopped = Arc::new(AtomicBool::new(false));
        // owner 的启动参数
        let mut setup = owner::Setup {
            // 根目录
            root: root.clone(),
            // 标识
            id,
            // 名称
            name,
            // 配置
            config,
            // 已发布状态
            published: published.clone(),
            // 修订号
            revision: revision.clone(),
            // 事件发送端
            events: tx.clone(),
            // 克隆停止标记
            stopped: stopped.clone(),
        };
        // 首份投影在返回前就绪，界面不必等待 owner 线程。
        // 首份投影在返回前就绪，界面不必等待 owner 线程
        owner::publish_initial(&setup);
        // 启动 owner 线程
        thread::Builder::new()
            // 线程名
            .name("loki-lan-owner".into())
            // 启动所有者线程
            .spawn(move || owner::run(&mut setup, rx))
            // 失败转为错误
            .map_err(err)?;
        // 构造句柄
        Ok(Self {
            // 发送端
            tx,
            // 修订号
            revision,
            // 命令意图
            intent: Mutex::new(CommandIntent::default()),
            // 已发布视图
            published,
            // 宿主生命周期标记
            host_lifecycle,
            // 停止标记
            stopped,
        })
    }
    /// 界面命令：长度有界、字段白名单，宿主专用操作不能从这里提交。
    // 界面命令：长度有界、字段白名单，宿主专用操作不能从这里提交
    pub fn command(&self, input: &str) -> Result<(), String> {
        // 参数过长
        if input.len() > 4096 {
            // 操作参数过长
            return Err("操作参数过长".into());
        }
        // 解析命令
        let command: Command =
            // 解析失败报无效连接操作
            serde_json::from_str(input).map_err(|_| "无效连接操作".to_owned())?;
        // 宿主已提供前后台时忽略界面的 active 命令
        if self.host_lifecycle && matches!(command, Command::Active { .. }) {
            // 无需入队
            return Ok(());
        }
        // 入队
        self.enqueue(command)
    }
    /// 宿主命令：前后台、网络变化、后台刷新与配套设备状态。
    #[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
    // 宿主命令：前后台、网络变化、后台刷新与配套设备状态
    pub fn host_command(&self, input: &str) -> Result<(), String> {
        // 命令过大
        if input.len() > 16 * 1024 {
            // 宿主命令过大
            return Err("宿主命令过大".into());
        }
        // 解析为宿主命令
        let command: HostCommand =
            // 解析失败报无效宿主命令
            serde_json::from_str(input).map_err(|_| "无效宿主命令".to_owned())?;
        // 作为宿主命令入队
        self.enqueue(Command::Host(command))
    }
    // 返回最近一次公开投影；锁中毒时仍取内部值
    pub fn poll(&self) -> String {
        // 已发布状态
        self.published
            // 状态
            .state
            // 加锁
            .lock()
            // 锁中毒时仍取出内部值
            .unwrap_or_else(|p| p.into_inner())
            // 视图
            .view
            // 复制
            .clone()
    }
}
#[cfg(test)]
// 句柄的方法
impl Handle {
    /// 不启动 owner 线程的句柄；测试直接消费事件队列。
    // 测试用：不启动 owner 线程，测试直接消费事件队列
    fn for_test(capacity: usize, selected: Option<&str>) -> (Self, mpsc::Receiver<Event>) {
        // 创建有界通道
        let (tx, rx) = mpsc::sync_channel(capacity);
        // 构造句柄
        let handle = Self {
            // 发送端
            tx,
            // 修订号
            revision: Arc::new(AtomicU64::new(0)),
            // 命令意图
            intent: Mutex::new(CommandIntent {
                // 选中的客户端
                selected: selected.map(str::to_owned),
                // 其余字段取默认值
                ..Default::default()
            }),
            // 默认已发布状态
            published: Arc::new(Published::default()),
            // 未启用宿主生命周期
            host_lifecycle: false,
            // 未停止
            stopped: Arc::new(AtomicBool::new(false)),
        };
        // 返回句柄与接收端
        (handle, rx)
    }
}
// 释放句柄时标记停止并通知 owner
impl Drop for Handle {
    // 销毁时
    fn drop(&mut self) {
        // 标记已停止
        self.stopped.store(true, Ordering::SeqCst);
        // 入队停止命令
        let _ = self.enqueue(Command::Stop);
    }
}

// 测试模块
#[cfg(test)]
#[path = "connection_test.rs"]
mod connection_test;
