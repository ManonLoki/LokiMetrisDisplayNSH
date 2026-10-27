//! 展示端连接入口和单线程 owner，桌面与移动端共用。
//! NSH submits bounded commands and reads public state; networking and storage belong to one owner thread.
mod app_visibility;
mod asset_cache;
mod assets;
mod commands;
mod discovery;
mod framed_socket;
mod owner;
mod pairing;
mod persistence;
mod public_view;
mod session;
mod snapshot_admission;
mod widget;

#[cfg(all(test, target_os = "macos"))]
mod companion_test;

use commands::{CommandIntent, QueuedCommand};
use loki_metris_pairing::{valid_device_id, valid_device_name};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

const SERVICE: &str = "_loki-host._tcp.local.";
const CREDENTIAL_SERVICE: &str = "com.manonloki.lokimetris.display.pairing.v2";
const MAX_FRAME: usize = 96 * 1024;
const HANDSHAKE_FRAME: usize = 4096;
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn err(_: impl std::fmt::Display) -> String {
    "本地连接或存储操作失败，请重试。".into()
}

/// 界面可提交的连接操作；宿主专用操作另见 [`HostCommand`]。
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
enum Command {
    Discover,
    Select {
        #[serde(rename = "clientId")]
        client_id: String,
    },
    Forget {
        #[serde(rename = "clientId")]
        client_id: String,
    },
    Rename {
        name: String,
    },
    /// Desktop visibility; mobile activity is supplied by the NSH system adapter.
    Active {
        value: bool,
    },
    Refresh,
    #[serde(skip)]
    #[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
    Host(HostCommand),
    #[serde(skip)]
    Stop,
}

/// System facts from the NSH runtime adapter, unavailable to JavaScript commands.
#[derive(Deserialize)]
#[serde(
    tag = "op",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum HostCommand {
    /// Application visibility and local-network permission.
    Lifecycle { visible: bool, permitted: bool },
    /// 系统网络路径变化后重新发现。
    Network,
}

/// NSH runtime configuration with the system identity on mobile targets.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Config {
    /// 应用私有目录；owner 在其下建立 `loki-link-v1`。
    pub root: PathBuf,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub name: String,
    /// The system credential store owns the mobile identity when true.
    #[serde(default)]
    pub host_identity: bool,
    /// The NSH system adapter supplies activity; UI active commands are ignored.
    #[serde(default)]
    pub host_lifecycle: bool,
    #[serde(default = "default_platform")]
    pub platform: String,
    /// 可信状态提交时为配对的 Apple Watch 发布公开缓存。
    #[serde(default)]
    pub watch_companion: bool,
    /// System-granted WidgetKit App Group; never supplied by JavaScript.
    #[serde(default)]
    pub widget_dir: Option<PathBuf>,
    #[serde(default = "default_version")]
    pub app_version: String,
    /// `phone` 或 `tablet`；桌面不上报形态。
    #[serde(default)]
    pub form_factor: Option<String>,
    /// 离线时是否保留并恢复最近快照。
    #[serde(default = "yes")]
    pub keep_snapshot: bool,
    /// 原生宿主旧存储中的面板偏好，只在 Rust 尚无保存值时导入一次。
    #[serde(default)]
    #[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
    pub legacy_panel: Option<serde_json::Value>,
}
fn default_platform() -> String {
    std::env::consts::OS.into()
}
fn default_version() -> String {
    "1.0.0".into()
}
fn yes() -> bool {
    true
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Material {
    client_id: String,
    device_id: String,
    name: String,
    secret: String,
    saved_at_ms: u64,
}

/// 发现得到的端点；`key` 为服务实例全名，同一桌面多个地址时以此区分。
#[derive(Clone)]
struct Endpoint {
    id: String,
    name: String,
    address: std::net::SocketAddr,
}

/// owner 的唯一事件入口：命令、连接线程与发现线程共享同一个队列。
enum Event {
    Command(QueuedCommand),
    Socket(u64, framed_socket::SocketEvent),
    Discovery(discovery::Found),
}

/// The latest public projection read by the NSH application.
#[derive(Default)]
struct Published {
    state: Mutex<PublishedState>,
}
#[derive(Default, Clone)]
struct PublishedState {
    view: String,
}

pub(crate) struct Handle {
    tx: SyncSender<Event>,
    revision: Arc<AtomicU64>,
    intent: Mutex<CommandIntent>,
    published: Arc<Published>,
    host_lifecycle: bool,
    stopped: Arc<AtomicBool>,
}
impl Handle {
    pub fn new(config: Config) -> Result<Self, String> {
        let root = config.root.join("loki-link-v1");
        persistence::private_dir(&root)?;
        let (id, name) = persistence::identity(&root, &config)?;
        if !valid_device_id(&id) || !valid_device_name(&name) {
            return Err("设备身份无效".into());
        }
        let published = Arc::new(Published::default());
        let (tx, rx) = mpsc::sync_channel(256);
        let revision = Arc::new(AtomicU64::new(0));
        let host_lifecycle = config.host_lifecycle;
        let stopped = Arc::new(AtomicBool::new(false));
        let mut setup = owner::Setup {
            root: root.clone(),
            id,
            name,
            config,
            published: published.clone(),
            revision: revision.clone(),
            events: tx.clone(),
            stopped: stopped.clone(),
        };
        // 首份投影在返回前就绪，界面不必等待 owner 线程。
        owner::publish_initial(&setup);
        thread::Builder::new()
            .name("loki-lan-owner".into())
            .spawn(move || owner::run(&mut setup, rx))
            .map_err(err)?;
        Ok(Self {
            tx,
            revision,
            intent: Mutex::new(CommandIntent::default()),
            published,
            host_lifecycle,
            stopped,
        })
    }
    /// 界面命令：长度有界、字段白名单，宿主专用操作不能从这里提交。
    pub fn command(&self, input: &str) -> Result<(), String> {
        if input.len() > 4096 {
            return Err("操作参数过长".into());
        }
        let command: Command =
            serde_json::from_str(input).map_err(|_| "无效连接操作".to_owned())?;
        if self.host_lifecycle && matches!(command, Command::Active { .. }) {
            return Ok(());
        }
        self.enqueue(command)
    }
    /// 宿主命令：前后台、网络变化、后台刷新与配套设备状态。
    #[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
    pub fn host_command(&self, input: &str) -> Result<(), String> {
        if input.len() > 16 * 1024 {
            return Err("宿主命令过大".into());
        }
        let command: HostCommand =
            serde_json::from_str(input).map_err(|_| "无效宿主命令".to_owned())?;
        self.enqueue(Command::Host(command))
    }
    pub fn poll(&self) -> String {
        self.published
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .view
            .clone()
    }
}
#[cfg(test)]
impl Handle {
    /// 不启动 owner 线程的句柄；测试直接消费事件队列。
    fn for_test(capacity: usize, selected: Option<&str>) -> (Self, mpsc::Receiver<Event>) {
        let (tx, rx) = mpsc::sync_channel(capacity);
        let handle = Self {
            tx,
            revision: Arc::new(AtomicU64::new(0)),
            intent: Mutex::new(CommandIntent {
                selected: selected.map(str::to_owned),
                ..Default::default()
            }),
            published: Arc::new(Published::default()),
            host_lifecycle: false,
            stopped: Arc::new(AtomicBool::new(false)),
        };
        (handle, rx)
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.enqueue(Command::Stop);
    }
}

#[cfg(test)]
#[path = "connection_test.rs"]
mod connection_test;
