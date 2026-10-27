//! 连接 owner 的状态与事件循环。
//! 循环只在事件到来或最近期限到达时醒来；空闲、暂停和后台时不周期唤醒。
use super::{
    Command, Config, Endpoint, Event, HostCommand, Material, Published, QueuedCommand,
    assets::AssetState,
    discovery::Discovery,
    framed_socket::{FramedSocket, Phase, SocketEvent},
    pairing::Protocol,
    persistence,
    public_view::empty_view,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError, SyncSender},
    },
    time::{Duration, Instant},
};

const IDLE_TIMEOUT: Duration = Duration::from_secs(45);

/// owner 线程启动所需的全部输入；创建失败后可按用户重试重新读取。
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

/// 宿主提供的生命周期事实。桌面仅以窗口可见性驱动。
#[derive(Clone, Copy)]
pub(super) struct Lifecycle {
    pub visible: bool,
    pub permitted: bool,
}

pub(super) struct Owner {
    pub root: PathBuf,
    pub id: String,
    pub name: String,
    pub platform: String,
    pub watch_companion: bool,
    pub widget: super::widget::WidgetShare,
    pub app_version: String,
    pub form_factor: Option<String>,
    pub keep_snapshot: bool,
    pub view: Value,
    pub published: Arc<Published>,
    pub dirty: bool,
    pub events: SyncSender<Event>,
    pub stopped: Arc<AtomicBool>,
    pub protocol: Protocol,
    pub revision: Arc<AtomicU64>,
    pub records: BTreeMap<String, Material>,
    pub endpoints: BTreeMap<String, Endpoint>,
    pub selected: Option<String>,
    pub discovery: Discovery,
    pub socket: Option<FramedSocket>,
    pub generation: u64,
    pub lifecycle: Lifecycle,
    pub pairing_offer: Option<Value>,
    pub pairing_deadline: Option<Instant>,
    pub retry_at: Option<Instant>,
    pub retries: u8,
    pub assets: AssetState,
    pub capabilities: BTreeSet<String>,
    pub credential_service: String,
    pub form_factor_reported: bool,
}

impl Owner {
    pub(super) fn new(setup: &Setup) -> Result<Self, String> {
        let config = &setup.config;
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
            generation: 0,
            lifecycle: Lifecycle {
                visible: !config.host_lifecycle,
                permitted: !config.host_lifecycle,
            },
            pairing_offer: None,
            pairing_deadline: None,
            retry_at: None,
            retries: 0,
            assets: AssetState::default(),
            capabilities: BTreeSet::new(),
            credential_service: super::CREDENTIAL_SERVICE.into(),
            form_factor_reported: false,
        };
        this.reconcile_widget_authorization();
        this.restore_cache();
        this.update_discovery();
        Ok(this)
    }

    /// 前台或用户开启的后台连接，且本地网络已授权。
    pub(super) fn active(&self) -> bool {
        self.lifecycle.permitted && self.lifecycle.visible
    }

    pub(super) fn run(&mut self, rx: Receiver<Event>) {
        loop {
            if self.stopped.load(Ordering::SeqCst) {
                break;
            }
            self.commit();
            let now = Instant::now();
            let timeout = self
                .next_deadline()
                .map_or(Duration::from_secs(3600), |at| {
                    at.saturating_duration_since(now)
                });
            match rx.recv_timeout(timeout) {
                Ok(Event::Command(QueuedCommand {
                    command: Command::Stop,
                    ..
                }))
                | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Event::Command(command)) => {
                    if let Err(e) = self.apply(command) {
                        self.view["error"] = json!(e);
                        self.publish();
                    }
                }
                Ok(Event::Socket(generation, event)) => {
                    if self
                        .socket
                        .as_ref()
                        .is_some_and(|w| w.generation == generation)
                    {
                        self.socket_event(event);
                    }
                }
                Ok(Event::Discovery(found)) => self.found(found),
                Err(RecvTimeoutError::Timeout) => {}
            }
            if self
                .protocol
                .pairing_revision()
                .is_some_and(|r| r != self.revision.load(Ordering::SeqCst))
            {
                self.cancel();
                self.view["status"] = json!("配对已取消，请重新选择桌面。");
                self.publish();
            }
            self.expire(Instant::now());
        }
        self.cancel();
        self.discovery.stop();
        self.commit();
    }

    fn next_deadline(&self) -> Option<Instant> {
        let socket = self.socket.as_ref().map(|w| {
            if w.phase == Phase::Streaming && !w.pausing {
                w.deadline.min(w.last_inbound + IDLE_TIMEOUT)
            } else {
                w.deadline
            }
        });
        [
            self.pairing_deadline,
            self.retry_at,
            socket,
            self.assets.deadline(),
            self.discovery.retry_at(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn expire(&mut self, now: Instant) {
        if self.pairing_deadline.is_some_and(|d| now >= d) {
            self.cancel();
            self.view["status"] = json!("暂未连接");
            self.view["error"] = json!("配对窗口已过期，请重新选择桌面。");
            self.publish();
        }
        if self.assets.deadline().is_some_and(|d| now >= d) {
            self.asset_timeout();
        }
        if self.discovery.retry_at().is_some_and(|d| now >= d) {
            self.discovery.clear_retry();
            self.update_discovery();
        }
        if let Some(socket) = &self.socket {
            if now >= socket.deadline {
                if socket.pausing {
                    self.drop_socket();
                } else {
                    self.failed("连接超时，正在重试…".into());
                }
            } else if socket.phase == Phase::Streaming
                && !socket.pausing
                && now.duration_since(socket.last_inbound) >= IDLE_TIMEOUT
            {
                self.failed("桌面暂不可达，恢复网络后重新连接。".into());
            }
        }
        if self.retry_at.is_some_and(|t| now >= t) {
            self.retry_at = None;
            if let Err(e) = self.connect() {
                self.failed(e);
            }
        }
    }

    fn socket_event(&mut self, event: SocketEvent) {
        match event {
            SocketEvent::Frame(value) => {
                if let Some(socket) = self.socket.as_mut() {
                    socket.last_inbound = Instant::now();
                    if socket.pausing {
                        return;
                    }
                }
                if let Err(e) = self.received(value) {
                    self.failed(e);
                    return;
                }
                if let Some(socket) = &self.socket {
                    socket.advance();
                }
            }
            SocketEvent::Drained => self.drained(),
            SocketEvent::Closed(reason) => {
                let Some(socket) = &self.socket else { return };
                if socket.paired_close && socket.drained() {
                    self.pairing_completed();
                } else if socket.pausing {
                    self.drop_socket();
                } else {
                    self.failed(reason);
                }
            }
        }
    }

    fn drained(&mut self) {
        let Some(socket) = &self.socket else { return };
        if socket.paired_close {
            self.pairing_completed();
        } else if socket.pausing {
            self.drop_socket();
        }
    }

    /// 保存 ACK 写完即结束配对连接，稍后以新 TCP 和已存秘密认证。
    fn pairing_completed(&mut self) {
        self.drop_socket();
        self.clear_pairing();
        self.retry_at = Some(Instant::now() + Duration::from_millis(500));
        self.publish();
    }

    /// 仅在内容变化后序列化一次，并唤醒等待投影的宿主。
    pub(super) fn publish(&mut self) {
        self.dirty = true;
    }

    pub(super) fn commit(&mut self) {
        if !std::mem::take(&mut self.dirty) {
            return;
        }
        let view = self.projection();
        if self.watch_companion {
            let source = self
                .selected
                .as_deref()
                .filter(|id| self.records.contains_key(*id));
            crate::native::companion::publish(
                &self.root,
                source,
                self.view["snapshot"]
                    .is_object()
                    .then_some(&self.view["snapshot"]),
                self.view["receivedAtMs"].as_u64(),
            );
        }
        let mut state = self
            .published
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if state.view == view {
            return;
        }
        state.view = view;
        drop(state);
    }
}

#[cfg(test)]
impl Owner {
    /// 不读写真实凭据位置的 owner：记录使用 `service` 指定的隔离安全存储条目。
    pub(super) fn for_test(
        root: PathBuf,
        id: &str,
        name: &str,
        service: &str,
        events: SyncSender<Event>,
    ) -> Self {
        persistence::private_dir(&root).unwrap();
        Self {
            root,
            id: id.into(),
            name: name.into(),
            platform: "ios".into(),
            watch_companion: true,
            widget: super::widget::WidgetShare::default(),
            app_version: "1.0.0".into(),
            form_factor: None,
            keep_snapshot: true,
            view: empty_view(id, name),
            published: Arc::new(Published::default()),
            dirty: true,
            events,
            stopped: Arc::new(AtomicBool::new(false)),
            protocol: Protocol::default(),
            revision: Arc::new(AtomicU64::new(0)),
            records: BTreeMap::new(),
            endpoints: BTreeMap::new(),
            selected: None,
            discovery: Discovery::disabled(),
            socket: None,
            generation: 0,
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
            credential_service: service.into(),
            form_factor_reported: false,
        }
    }

    /// 测试读取最近一次提交的公开投影。
    pub(super) fn published_view(&mut self) -> Value {
        self.commit();
        let state = self.published.state.lock().unwrap();
        serde_json::from_str(&state.view).unwrap()
    }
}

/// 在 owner 线程启动前发布空投影，界面初始化无需等待凭据读取。
pub(super) fn publish_initial(setup: &Setup) {
    let view = empty_view(&setup.id, &setup.name);
    let mut state = setup
        .published
        .state
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    state.view = view.to_string();
}

/// owner 线程入口。安全存储不可用时保留错误投影，等待用户重试或宿主恢复前台。
pub(super) fn run(setup: &mut Setup, rx: Receiver<Event>) {
    // 存储不可用期间到达的最新生命周期事实，在 owner 就绪后补上。
    let mut lifecycle: Option<HostCommand> = None;
    loop {
        if setup.stopped.load(Ordering::SeqCst) {
            return;
        }
        match Owner::new(setup) {
            Ok(mut owner) => {
                if let Some(command) = lifecycle.take() {
                    let _ = owner.host(command);
                }
                owner.run(rx);
                return;
            }
            Err(error) => {
                let mut view = empty_view(&setup.id, &setup.name);
                view["error"] = json!(error);
                view["status"] = json!("安全存储或连接不可用，请解锁后重试");
                let mut state = setup
                    .published
                    .state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                state.view = view.to_string();
                drop(state);
            }
        }
        loop {
            if setup.stopped.load(Ordering::SeqCst) {
                return;
            }
            match rx.recv() {
                Ok(Event::Command(QueuedCommand {
                    command: Command::Host(command @ HostCommand::Lifecycle { .. }),
                    ..
                })) => {
                    lifecycle = Some(command);
                    break;
                }
                Ok(Event::Command(QueuedCommand {
                    command: Command::Refresh | Command::Discover,
                    ..
                })) => break,
                Ok(Event::Command(QueuedCommand {
                    command: Command::Stop,
                    ..
                }))
                | Err(_) => return,
                _ => {}
            }
        }
    }
}
