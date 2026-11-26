//! 展示端连接入口和单线程 owner，桌面与移动端共用。
//! NSH 提交有限的命令并读取公开状态;网络与存储归一个所有者线程负责。
mod asset_cache;
mod commands;
mod discovery;
mod framed_socket;
mod owner;
mod pairing;
mod persistence;
mod snapshot_admission;

use super::bootstrap::Config;
use commands::{CommandIntent, QueuedCommand};
use loki_metris_pairing::{valid_device_id, valid_device_name};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        // 原子引用计数与互斥锁
        Arc,
        // 互斥锁
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    // 线程
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

// 桌面广播的服务类型
const SERVICE: &str = "_loki-host._tcp.local.";
// 系统凭据库中的服务名
const CREDENTIAL_SERVICE: &str = "com.manonloki.lokimetris.display.pairing.v2";
const MAX_FRAME: usize = 96 * 1024;
const HANDSHAKE_FRAME: usize = 4096;
// 当前 Unix 毫秒时间
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
// 把任意错误统一为面向用户的文案，不泄露细节
fn err(_: impl std::fmt::Display) -> String {
    // 统一的本地失败提示
    "本地连接或存储操作失败，请重试。".into()
}

/// 连接 owner 的命令。宿主和停止命令仅在 Rust 内部构造。
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
    /// 界面是否可见，由界面按 NSH 的 `lifecycle()` 报告。
    Active {
        value: bool,
    },
    Refresh,
    #[serde(skip)]
    // 停止 owner
    Stop,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
// 配对材料，保存到系统凭据库
struct Material {
    client_id: String,
    device_id: String,
    name: String,
    secret: String,
    saved_at_ms: u64,
}

/// 发现得到的端点；`key` 为服务实例全名，同一桌面多个地址时以此区分。
#[derive(Clone)]
// 发现得到的端点：身份、名称与地址
struct Endpoint {
    id: String,
    name: String,
    address: std::net::SocketAddr,
}

/// owner 的唯一事件入口：命令、连接线程与发现线程共享同一个队列。
// owner 的唯一事件入口：命令、连接线程与发现线程共用同一队列
enum Event {
    Command(QueuedCommand),
    Socket(u64, framed_socket::SocketEvent),
    Discovery(discovery::Found),
    // 原生WC实际回执，仍由当前连接owner独占加密发送。
    #[cfg(any(target_os = "ios", test))]
    Companion {
        generation: u64,
        scope: String,
        nonce: u64,
        report: crate::native::companion::presence::WireReport,
    },
}

/// NSH 应用读取的最新公开投影。
#[derive(Default)]
// 最近一次公开投影，供 NSH 应用读取
pub(super) struct Published {
    // 已发布状态（加锁）
    state: Mutex<PublishedState>,
}
#[derive(Default, Clone)]
struct PublishedState {
    // 视图 JSON 文本
    view: String,
    // Shell 每个连接只有一个等待下一份视图的任务。
    waiter: Option<std::task::Waker>,
    closed: bool,
}

impl Published {
    fn update(&self, view: String) {
        let waiter = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.view == view {
                return;
            }
            state.view = view;
            state.waiter.take()
        };
        if let Some(waiter) = waiter {
            waiter.wake();
        }
    }

    pub(super) async fn wait_for_change(&self, previous: &str) -> bool {
        use std::task::Poll;
        std::future::poll_fn(|cx| {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.closed {
                return Poll::Ready(false);
            }
            if state.view != previous {
                return Poll::Ready(true);
            }
            state.waiter = Some(cx.waker().clone());
            Poll::Pending
        })
        .await
    }

    fn close(&self) {
        let waiter = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.closed = true;
            state.waiter.take()
        };
        if let Some(waiter) = waiter {
            waiter.wake();
        }
    }
}

// 连接句柄：向 owner 投递命令并读取公开状态
pub(crate) struct Handle {
    tx: SyncSender<Event>,
    revision: Arc<AtomicU64>,
    intent: Mutex<CommandIntent>,
    published: Arc<Published>,
    stopped: Arc<AtomicBool>,
}
impl Handle {
    // 创建句柄并启动 owner 线程
    pub(super) fn new(config: Config) -> Result<Self, String> {
        let root = config.root.join("loki-link-v1");
        // 确保根目录为私有目录
        persistence::private_dir(&root)?;
        // 读取或生成设备身份并校验
        let (id, name) = persistence::identity(&root, &config)?;
        // 设备标识或名称不合法
        if !valid_device_id(&id) || !valid_device_name(&name) {
            return Err("设备身份无效".into());
        }
        let published = Arc::new(Published::default());
        // 容量 256 的有界队列
        let (tx, rx) = mpsc::sync_channel(256);
        let revision = Arc::new(AtomicU64::new(0));
        let stopped = Arc::new(AtomicBool::new(false));
        // owner 的启动参数
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
        // 首份投影在返回前就绪，界面不必等待 owner 线程
        owner::publish_initial(&setup);
        // 启动 owner 线程
        thread::Builder::new()
            // 线程名
            .name("loki-lan-owner".into())
            // 启动所有者线程
            .spawn(move || owner::run(&mut setup, rx))
            .map_err(err)?;
        Ok(Self {
            tx,
            revision,
            intent: Mutex::new(CommandIntent::default()),
            published,
            stopped,
        })
    }
    /// 界面命令：长度有界、字段白名单。
    // 界面命令：长度有界、字段白名单，宿主专用操作不能从这里提交
    pub fn command(&self, input: &str) -> Result<(), String> {
        if input.len() > 4096 {
            return Err("操作参数过长".into());
        }
        let command: Command =
            // 解析失败报无效连接操作
            serde_json::from_str(input).map_err(|_| "无效连接操作".to_owned())?;
        self.enqueue(command)
    }
    pub(super) fn publications(&self) -> Arc<Published> {
        self.published.clone()
    }

    // 返回最近一次公开投影；锁中毒时仍取内部值
    pub fn poll(&self) -> String {
        self.published
            .state
            // 加锁
            .lock()
            // 锁中毒时仍取出内部值
            .unwrap_or_else(|p| p.into_inner())
            .view
            .clone()
    }
}
#[cfg(test)]
impl Handle {
    /// 不启动 owner 线程的句柄；测试直接消费事件队列。
    // 测试用：不启动 owner 线程，测试直接消费事件队列
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
            stopped: Arc::new(AtomicBool::new(false)),
        };
        (handle, rx)
    }
}
// 释放句柄时标记停止并通知 owner
impl Drop for Handle {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.published.close();
        let _ = self.enqueue(Command::Stop);
    }
}
