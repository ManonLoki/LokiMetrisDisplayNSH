//! 连接 owner 的状态与事件循环。
//! 循环只在事件到来或最近期限到达时醒来；空闲、暂停和后台时不周期唤醒。
use super::{
    // 命令、配置、端点、事件等类型
    Command,
    Config,
    Endpoint,
    Event,
    Material,
    Published,
    QueuedCommand,
    assets::AssetState,
    discovery::Discovery,
    // 带帧套接字及其阶段与事件
    framed_socket::{FramedSocket, Phase, SocketEvent},
    pairing::Protocol,
    persistence,
    public_view::empty_view,
};
use serde_json::{Value, json};
use std::{
    // 有序映射与有序集合
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        // 通道的接收端、超时错误与发送端
        mpsc::{Receiver, RecvTimeoutError, SyncSender},
    },
    time::{Duration, Instant},
};

mod events;
#[cfg(test)]
mod test_support;

mod run_loop;

// 推送阶段 45 秒没有入站数据即视为超时
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);

/// owner 线程启动所需的全部输入；创建失败后可按用户重试重新读取。
// owner 线程启动所需的全部输入；创建失败后可按用户重试重新读取
pub(super) struct Setup {
    pub root: PathBuf,
    pub id: String,
    pub name: String,
    pub config: Config,
    pub published: Arc<Published>,
    pub revision: Arc<AtomicU64>,
    pub events: SyncSender<Event>,
    pub stopped: Arc<AtomicBool>,
}

/// 界面报告的生命周期事实。
#[derive(Clone, Copy)]
// 界面报告的生命周期事实
pub(super) struct Lifecycle {
    pub visible: bool,
    // 本地网络是否已授权
    pub permitted: bool,
}

// owner 的全部状态，只在 owner 线程内使用
pub(super) struct Owner {
    pub root: PathBuf,
    pub id: String,
    pub name: String,
    pub platform: String,
    // 是否为手表发布缓存
    pub watch_companion: bool,
    pub widget: super::widget::WidgetShare,
    pub app_version: String,
    pub form_factor: Option<String>,
    pub keep_snapshot: bool,
    pub view: Value,
    pub published: Arc<Published>,
    // 视图是否有待提交的变化
    pub dirty: bool,
    pub events: SyncSender<Event>,
    pub stopped: Arc<AtomicBool>,
    // 配对与加密协议状态机
    pub protocol: Protocol,
    pub revision: Arc<AtomicU64>,
    pub records: BTreeMap<String, Material>,
    pub endpoints: BTreeMap<String, Endpoint>,
    pub selected: Option<String>,
    pub discovery: Discovery,
    pub socket: Option<FramedSocket>,
    // 连接代数，用于忽略旧连接的事件
    pub generation: u64,
    pub lifecycle: Lifecycle,
    pub pairing_offer: Option<Value>,
    pub pairing_deadline: Option<Instant>,
    pub retry_at: Option<Instant>,
    pub retries: u8,
    pub assets: AssetState,
    pub capabilities: BTreeSet<String>,
    pub credential_service: String,
    // 本连接是否已上报设备形态
    pub form_factor_reported: bool,
    // 当前LAN的WC回调资格与真实桌面ping锚。
    #[cfg(any(target_os = "ios", test))]
    pub watch: super::watch::Relay,
}

impl Owner {
    // 构造 owner：读取已保存的配对记录并恢复缓存
    pub(super) fn new(setup: &Setup) -> Result<Self, String> {
        let config = &setup.config;
        // 凭据库不可用时在此失败
        let records = persistence::load_records(super::CREDENTIAL_SERVICE, &setup.id)?;
        let mut this = Self {
            root: setup.root.clone(),
            id: setup.id.clone(),
            name: setup.name.clone(),
            platform: config.platform.clone(),
            watch_companion: config.watch_companion,
            widget: super::widget::WidgetShare::new(config.widget_dir.clone()),
            app_version: config.app_version.clone(),
            form_factor: config.form_factor.clone(),
            keep_snapshot: config.keep_snapshot,
            view: empty_view(&setup.id, &setup.name),
            published: setup.published.clone(),
            // 首次必须提交
            dirty: true,
            events: setup.events.clone(),
            stopped: setup.stopped.clone(),
            protocol: Protocol::default(),
            revision: setup.revision.clone(),
            records,
            endpoints: BTreeMap::new(),
            selected: None,
            discovery: Discovery::default(),
            socket: None,
            // 连接代次从 0 开始
            generation: 0,
            // 界面报告可见性之前按可见处理
            lifecycle: Lifecycle {
                visible: true,
                permitted: true,
            },
            pairing_offer: None,
            pairing_deadline: None,
            retry_at: None,
            retries: 0,
            assets: AssetState::default(),
            capabilities: BTreeSet::new(),
            credential_service: super::CREDENTIAL_SERVICE.into(),
            form_factor_reported: false,
            #[cfg(any(target_os = "ios", test))]
            watch: Default::default(),
        };
        this.reconcile_widget_authorization();
        // 恢复本地缓存的快照
        this.restore_cache();
        this.update_discovery();
        Ok(this)
    }

    /// 前台或用户开启的后台连接，且本地网络已授权。
    // 前台或用户开启的后台连接，且本地网络已授权
    pub(super) fn active(&self) -> bool {
        // 允许且可见时才算活动
        self.lifecycle.permitted && self.lifecycle.visible
    }

    /// 仅在内容变化后序列化一次，并唤醒等待投影的宿主。
    // 标记视图有变化，下一轮循环统一序列化
    pub(super) fn publish(&mut self) {
        self.dirty = true;
    }

    // 仅在内容变化后序列化一次并提交
    pub(super) fn commit(&mut self) {
        if !std::mem::take(&mut self.dirty) {
            return;
        }
        let view = self.projection();
        // 同步给手表的公开缓存（只用有配对记录的来源）
        if self.watch_companion {
            let source = self
                .selected
                .as_deref()
                // 必须在配对记录里
                .filter(|id| self.records.contains_key(*id));
            crate::native::companion::publish(
                &self.root,
                source,
                self.view["snapshot"]
                    // 仅当其为对象时
                    .is_object()
                    .then_some(&self.view["snapshot"]),
                self.view["receivedAtMs"].as_u64(),
            );
        }
        self.published.update(view);
    }
}

/// 在 owner 线程启动前发布空投影，界面初始化无需等待凭据读取。
// 在 owner 线程启动前发布空投影，界面初始化无需等待凭据读取
pub(super) fn publish_initial(setup: &Setup) {
    let view = empty_view(&setup.id, &setup.name);
    setup.published.update(view.to_string());
}

/// owner 线程入口。安全存储不可用时保留错误投影，等待用户重试或宿主恢复前台。
// owner 线程入口；安全存储不可用时保留错误投影，等待重试或宿主恢复前台
pub(super) fn run(setup: &mut Setup, rx: Receiver<Event>) {
    // 存储不可用期间到达的最新生命周期事实，在 owner 就绪后补上。
    // 存储不可用期间最新的生命周期事实，owner 就绪后补上
    let mut lifecycle: Option<Lifecycle> = None;
    // 外层循环：构造 owner，失败则等待重试
    loop {
        if setup.stopped.load(Ordering::SeqCst) {
            return;
        }
        match Owner::new(setup) {
            // 构造成功：补上生命周期后进入事件循环
            Ok(mut owner) => {
                if let Some(lifecycle) = lifecycle.take() {
                    owner.set_lifecycle(lifecycle);
                }
                owner.run(rx);
                return;
            }
            // 构造失败：发布错误投影
            Err(error) => {
                // 创建失败：构造空视图
                let mut view = empty_view(&setup.id, &setup.name);
                view["error"] = json!(error);
                // 状态：安全存储或连接不可用
                view["status"] = json!("安全存储或连接不可用，请解锁后重试");
                setup.published.update(view.to_string());
            }
        }
        // 内层循环：等待重试信号
        loop {
            if setup.stopped.load(Ordering::SeqCst) {
                return;
            }
            match rx.recv() {
                Ok(Event::Command(QueuedCommand {
                    command: Command::Active { value },
                    ..
                })) => {
                    lifecycle = Some(Lifecycle {
                        visible: value,
                        permitted: true,
                    });
                    break;
                }
                Ok(Event::Command(QueuedCommand {
                    // 用户刷新或重新发现：重试
                    command: Command::Refresh | Command::Discover,
                    ..
                })) => break,
                Ok(Event::Command(QueuedCommand {
                    // 停止或发送端断开：退出
                    command: Command::Stop,
                    ..
                }))
                | Err(_) => return,
                _ => {}
            }
        }
    }
}
