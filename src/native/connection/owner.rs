//! 连接 owner 的状态与事件循环。
//! 循环只在事件到来或最近期限到达时醒来；空闲、暂停和后台时不周期唤醒。
// 引入命令、配置、事件、材料、协议、发现、资源与套接字
use super::{
    // 命令、配置、端点、事件等类型
    Command,
    // 配置
    Config,
    // 端点
    Endpoint,
    // 事件
    Event,
    // 宿主命令
    HostCommand,
    // 密钥材料
    Material,
    // 已发布状态
    Published,
    // 排队命令
    QueuedCommand,
    // 资源状态
    assets::AssetState,
    // 发现状态
    discovery::Discovery,
    // 带帧套接字及其阶段与事件
    framed_socket::{FramedSocket, Phase, SocketEvent},
    // 配对协议
    pairing::Protocol,
    // 持久化
    persistence,
    // 构造空视图
    public_view::empty_view,
};
// 引入 JSON
use serde_json::{Value, json};
// 引入集合、路径、原子量、通道与时间
use std::{
    // 有序映射与有序集合
    collections::{BTreeMap, BTreeSet},
    // 路径类型
    path::PathBuf,
    // 同步原语
    sync::{
        // 原子引用计数
        Arc,
        // 原子类型与内存序
        atomic::{AtomicBool, AtomicU64, Ordering},
        // 通道的接收端、超时错误与发送端
        mpsc::{Receiver, RecvTimeoutError, SyncSender},
    },
    // 时长与时刻
    time::{Duration, Instant},
};

// 推送阶段 45 秒没有入站数据即视为超时
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);

/// owner 线程启动所需的全部输入；创建失败后可按用户重试重新读取。
// owner 线程启动所需的全部输入；创建失败后可按用户重试重新读取
pub(super) struct Setup {
    // 根目录
    pub root: PathBuf,
    // 设备 ID
    pub id: String,
    // 设备名称
    pub name: String,
    // 配置
    pub config: Config,
    // 公开投影
    pub published: Arc<Published>,
    // 取消代数
    pub revision: Arc<AtomicU64>,
    // 事件队列发送端
    pub events: SyncSender<Event>,
    // 停止标记
    pub stopped: Arc<AtomicBool>,
}

/// 宿主提供的生命周期事实。桌面仅以窗口可见性驱动。
#[derive(Clone, Copy)]
// 宿主提供的生命周期事实；桌面仅以窗口可见性驱动
pub(super) struct Lifecycle {
    // 是否可见
    pub visible: bool,
    // 本地网络是否已授权
    pub permitted: bool,
}

// owner 的全部状态，只在 owner 线程内使用
pub(super) struct Owner {
    // 根目录
    pub root: PathBuf,
    // 设备标识
    pub id: String,
    // 设备名
    pub name: String,
    // 上报给桌面的平台
    pub platform: String,
    // 是否为手表发布缓存
    pub watch_companion: bool,
    // 小组件共享
    pub widget: super::widget::WidgetShare,
    // 应用版本
    pub app_version: String,
    // 设备形态
    pub form_factor: Option<String>,
    // 是否保留快照
    pub keep_snapshot: bool,
    // 当前公开视图
    pub view: Value,
    // 已发布状态
    pub published: Arc<Published>,
    // 视图是否有待提交的变化
    pub dirty: bool,
    // 事件发送端
    pub events: SyncSender<Event>,
    // 停止标记
    pub stopped: Arc<AtomicBool>,
    // 配对与加密协议状态机
    pub protocol: Protocol,
    // 修订号
    pub revision: Arc<AtomicU64>,
    // 已保存的配对记录
    pub records: BTreeMap<String, Material>,
    // 已发现的端点
    pub endpoints: BTreeMap<String, Endpoint>,
    // 当前选中的桌面
    pub selected: Option<String>,
    // 发现任务
    pub discovery: Discovery,
    // 当前连接
    pub socket: Option<FramedSocket>,
    // 连接代数，用于忽略旧连接的事件
    pub generation: u64,
    // 生命周期状态
    pub lifecycle: Lifecycle,
    // 配对邀请
    pub pairing_offer: Option<Value>,
    // 配对窗口截止时间
    pub pairing_deadline: Option<Instant>,
    // 下次重连时间
    pub retry_at: Option<Instant>,
    // 已连续重试次数
    pub retries: u8,
    // 图片资源传输状态
    pub assets: AssetState,
    // 桌面声明的能力
    pub capabilities: BTreeSet<String>,
    // 凭据库服务名
    pub credential_service: String,
    // 本连接是否已上报设备形态
    pub form_factor_reported: bool,
}

// 所有者的方法
impl Owner {
    // 构造 owner：读取已保存的配对记录并恢复缓存
    pub(super) fn new(setup: &Setup) -> Result<Self, String> {
        // 取配置引用
        let config = &setup.config;
        // 凭据库不可用时在此失败
        let records = persistence::load_records(super::CREDENTIAL_SERVICE, &setup.id)?;
        // 构造所有者
        let mut this = Self {
            // 根目录
            root: setup.root.clone(),
            // 设备标识
            id: setup.id.clone(),
            // 设备名
            name: setup.name.clone(),
            // 平台
            platform: config.platform.clone(),
            // 是否启用手表配套
            watch_companion: config.watch_companion,
            // 小组件共享目录
            widget: super::widget::WidgetShare::new(config.widget_dir.clone()),
            // 应用版本
            app_version: config.app_version.clone(),
            // 设备形态
            form_factor: config.form_factor.clone(),
            // 是否保留快照
            keep_snapshot: config.keep_snapshot,
            // 初始为空视图
            view: empty_view(&setup.id, &setup.name),
            // 已发布状态
            published: setup.published.clone(),
            // 首次必须提交
            dirty: true,
            // 事件发送端
            events: setup.events.clone(),
            // 停止标记
            stopped: setup.stopped.clone(),
            // 默认协议状态
            protocol: Protocol::default(),
            // 修订号
            revision: setup.revision.clone(),
            // 已读取的配对记录
            records,
            // 端点为空
            endpoints: BTreeMap::new(),
            // 尚无选中项
            selected: None,
            // 默认的发现状态
            discovery: Discovery::default(),
            // 尚无套接字
            socket: None,
            // 连接代次从 0 开始
            generation: 0,
            // 宿主提供生命周期时，等其推送前保持不可见
            lifecycle: Lifecycle {
                // 由宿主生命周期决定初始可见性
                visible: !config.host_lifecycle,
                // 由宿主生命周期决定是否允许
                permitted: !config.host_lifecycle,
            },
            // 无配对邀请
            pairing_offer: None,
            // 无配对期限
            pairing_deadline: None,
            // 无重试时刻
            retry_at: None,
            // 重试次数为 0
            retries: 0,
            // 默认资源状态
            assets: AssetState::default(),
            // 能力集合为空
            capabilities: BTreeSet::new(),
            // 凭据服务名
            credential_service: super::CREDENTIAL_SERVICE.into(),
            // 尚未上报设备形态
            form_factor_reported: false,
        };
        // 对账小组件授权
        this.reconcile_widget_authorization();
        // 恢复本地缓存的快照
        this.restore_cache();
        // 按需启动发现
        this.update_discovery();
        // 返回所有者
        Ok(this)
    }

    /// 前台或用户开启的后台连接，且本地网络已授权。
    // 前台或用户开启的后台连接，且本地网络已授权
    pub(super) fn active(&self) -> bool {
        // 允许且可见时才算活动
        self.lifecycle.permitted && self.lifecycle.visible
    }

    // 事件循环：事件到来或最近期限到达才醒来，空闲、暂停、后台时不周期唤醒
    pub(super) fn run(&mut self, rx: Receiver<Event>) {
        // 主循环
        loop {
            // 句柄释放后退出
            if self.stopped.load(Ordering::SeqCst) {
                // 结束循环
                break;
            }
            // 提交上一轮的变化
            self.commit();
            // 当前时刻
            let now = Instant::now();
            // 等到最近的期限，没有期限则最长等一小时
            let timeout = self
                // 下一个期限
                .next_deadline()
                // 换算为等待时长，没有期限则等一小时
                .map_or(Duration::from_secs(3600), |at| {
                    // 距最近期限的等待时间
                    at.saturating_duration_since(now)
                });
            // 等待命令或超时
            match rx.recv_timeout(timeout) {
                // 收到停止命令或发送端全部断开：退出
                Ok(Event::Command(QueuedCommand {
                    // 通道超时：当作停止命令
                    command: Command::Stop,
                    // 其余字段忽略
                    ..
                }))
                // 通道断开也跳出循环
                | Err(RecvTimeoutError::Disconnected) => break,
                // 普通命令：失败则展示错误
                Ok(Event::Command(command)) => {
                    // 应用命令，失败则
                    if let Err(e) = self.apply(command) {
                        // 把错误写入视图
                        self.view["error"] = json!(e);
                        // 发布视图
                        self.publish();
                    }
                }
                // 套接字事件：只处理当前代数的连接
                Ok(Event::Socket(generation, event)) => {
                    // 判断是否应退出
                    if self
                        // 套接字
                        .socket
                        // 取引用
                        .as_ref()
                        // 代次一致
                        .is_some_and(|w| w.generation == generation)
                    {
                        // 处理套接字事件
                        self.socket_event(event);
                    }
                }
                // 发现到桌面
                Ok(Event::Discovery(found)) => self.found(found),
                // 超时只为检查期限
                Err(RecvTimeoutError::Timeout) => {}
            }
            // 配对的取消代数已变化：取消配对
            if self
                // 协议
                .protocol
                // 配对修订号
                .pairing_revision()
                // 与当前修订号不同
                .is_some_and(|r| r != self.revision.load(Ordering::SeqCst))
            {
                // 取消配对
                self.cancel();
                // 状态：配对已取消
                self.view["status"] = json!("配对已取消，请重新选择桌面。");
                // 发布视图
                self.publish();
            }
            // 处理所有到期的期限
            self.expire(Instant::now());
        }
        // 退出前清理并提交最后的状态
        self.cancel();
        // 停止发现
        self.discovery.stop();
        // 提交变更
        self.commit();
    }

    // 最近的期限：配对窗口、重连、套接字、资源传输与发现退避中最早的一个
    fn next_deadline(&self) -> Option<Instant> {
        // 推送阶段还要考虑空闲超时
        let socket = self.socket.as_ref().map(|w| {
            // 传输中且未暂停
            if w.phase == Phase::Streaming && !w.pausing {
                // 取期限与空闲超时中较早者
                w.deadline.min(w.last_inbound + IDLE_TIMEOUT)
            // 否则分支
            } else {
                // 否则使用期限
                w.deadline
            }
        });
        // 各类期限数组
        [
            // 配对期限
            self.pairing_deadline,
            // 重试时刻
            self.retry_at,
            // 套接字期限
            socket,
            // 资源期限
            self.assets.deadline(),
            // 发现重试时刻
            self.discovery.retry_at(),
        ]
        // 遍历存在的期限
        .into_iter()
        // 展平
        .flatten()
        // 取最早的
        .min()
    }

    // 处理已到期的期限
    fn expire(&mut self, now: Instant) {
        // 配对窗口过期：取消并提示
        if self.pairing_deadline.is_some_and(|d| now >= d) {
            // 取消配对
            self.cancel();
            // 状态：暂未连接
            self.view["status"] = json!("暂未连接");
            // 错误：配对窗口已过期
            self.view["error"] = json!("配对窗口已过期，请重新选择桌面。");
            // 发布视图
            self.publish();
        }
        // 资源传输超时
        if self.assets.deadline().is_some_and(|d| now >= d) {
            // 处理资源超时
            self.asset_timeout();
        }
        // 发现退避结束：重新启动发现
        if self.discovery.retry_at().is_some_and(|d| now >= d) {
            // 清除发现重试
            self.discovery.clear_retry();
            // 更新发现状态
            self.update_discovery();
        }
        // 套接字期限
        if let Some(socket) = &self.socket {
            // 超过期限：暂停握手中的直接断开，其余视为超时失败
            if now >= socket.deadline {
                // 已暂停
                if socket.pausing {
                    // 丢弃套接字
                    self.drop_socket();
                // 否则
                } else {
                    // 报连接超时并重试
                    self.failed("连接超时，正在重试…".into());
                }
            // 推送阶段长时间没有入站数据
            } else if socket.phase == Phase::Streaming
                // 并且不在暂停中
                && !socket.pausing
                // 并且空闲时间超过上限
                && now.duration_since(socket.last_inbound) >= IDLE_TIMEOUT
            {
                // 桌面暂不可达
                self.failed("桌面暂不可达，恢复网络后重新连接。".into());
            }
        }
        // 到了重连时间：发起连接，失败则继续退避
        if self.retry_at.is_some_and(|t| now >= t) {
            // 清除重试时刻
            self.retry_at = None;
            // 重新连接
            if let Err(e) = self.connect() {
                // 失败则记录
                self.failed(e);
            }
        }
    }

    // 处理套接字事件
    fn socket_event(&mut self, event: SocketEvent) {
        // 按事件类型处理
        match event {
            // 收到一帧
            SocketEvent::Frame(value) => {
                // 记录入站时间；暂停握手期间丢弃内容
                if let Some(socket) = self.socket.as_mut() {
                    // 记录最近一次收到数据的时刻
                    socket.last_inbound = Instant::now();
                    // 已暂停
                    if socket.pausing {
                        // 忽略
                        return;
                    }
                }
                // 处理失败则断开并重试
                if let Err(e) = self.received(value) {
                    // 处理出错
                    self.failed(e);
                    // 返回
                    return;
                }
                // 处理成功后继续读取下一帧
                if let Some(socket) = &self.socket {
                    // 推进套接字状态
                    socket.advance();
                }
            }
            // 发送队列已清空
            SocketEvent::Drained => self.drained(),
            // 连接关闭
            SocketEvent::Closed(reason) => {
                // 没有套接字则返回
                let Some(socket) = &self.socket else { return };
                // 配对确认已发完：配对完成
                if socket.paired_close && socket.drained() {
                    // 配对完成
                    self.pairing_completed();
                // 暂停中：丢弃连接
                } else if socket.pausing {
                    // 丢弃套接字
                    self.drop_socket();
                // 否则视为失败
                } else {
                    // 记录失败原因
                    self.failed(reason);
                }
            }
        }
    }

    // 发送队列清空后的收尾
    fn drained(&mut self) {
        // 没有套接字则返回
        let Some(socket) = &self.socket else { return };
        // 配对后关闭
        if socket.paired_close {
            // 配对完成
            self.pairing_completed();
        // 已暂停
        } else if socket.pausing {
            // 丢弃套接字
            self.drop_socket();
        }
    }

    /// 保存 ACK 写完即结束配对连接，稍后以新 TCP 和已存秘密认证。
    // 保存确认写完即结束配对连接，稍后用新的 TCP 与已保存的秘密认证
    fn pairing_completed(&mut self) {
        // 丢弃套接字
        self.drop_socket();
        // 清除配对状态
        self.clear_pairing();
        // 500 毫秒后重连
        self.retry_at = Some(Instant::now() + Duration::from_millis(500));
        // 发布视图
        self.publish();
    }

    /// 仅在内容变化后序列化一次，并唤醒等待投影的宿主。
    // 标记视图有变化，下一轮循环统一序列化
    pub(super) fn publish(&mut self) {
        // 标记需要发布
        self.dirty = true;
    }

    // 仅在内容变化后序列化一次并提交
    pub(super) fn commit(&mut self) {
        // 没有变化则跳过
        if !std::mem::take(&mut self.dirty) {
            // 不需要发布则返回
            return;
        }
        // 计算当前投影
        let view = self.projection();
        // 同步给手表的公开缓存（只用有配对记录的来源）
        if self.watch_companion {
            // 当前来源
            let source = self
                // 当前选中项
                .selected
                // 取字符串
                .as_deref()
                // 必须在配对记录里
                .filter(|id| self.records.contains_key(*id));
            // 发布给配套应用
            crate::native::companion::publish(
                // 根目录
                &self.root,
                // 来源
                source,
                // 快照对象
                self.view["snapshot"]
                    // 仅当其为对象时
                    .is_object()
                    // 才传入
                    .then_some(&self.view["snapshot"]),
                // 接收时间
                self.view["receivedAtMs"].as_u64(),
            );
        }
        // 与上一次相同则不更新
        let mut state = self
            // 已发布状态
            .published
            // 内部状态
            .state
            // 加锁
            .lock()
            // 锁中毒时仍取出内部值
            .unwrap_or_else(|p| p.into_inner());
        // 视图没有变化
        if state.view == view {
            // 不重复发布
            return;
        }
        // 更新视图
        state.view = view;
        // 释放锁
        drop(state);
    }
}

// 测试辅助
#[cfg(test)]
impl Owner {
    /// 不读写真实凭据位置的 owner：记录使用 `service` 指定的隔离安全存储条目。
    // 不读写真实凭据位置的 owner：记录使用 service 指定的隔离条目
    pub(super) fn for_test(
        // 根目录
        root: PathBuf,
        // 设备标识
        id: &str,
        // 设备名
        name: &str,
        // 凭据服务名
        service: &str,
        // 事件发送端
        events: SyncSender<Event>,
        // 返回所有者
    ) -> Self {
        // 创建私有目录
        persistence::private_dir(&root).unwrap();
        // 构造所有者
        Self {
            // 根目录
            root,
            // 设备标识
            id: id.into(),
            // 设备名
            name: name.into(),
            // 平台为 iOS
            platform: "ios".into(),
            // 启用手表配套
            watch_companion: true,
            // 默认小组件共享
            widget: super::widget::WidgetShare::default(),
            // 默认应用版本
            app_version: "1.0.0".into(),
            // 无设备形态
            form_factor: None,
            // 保留快照
            keep_snapshot: true,
            // 空视图
            view: empty_view(id, name),
            // 默认已发布状态
            published: Arc::new(Published::default()),
            // 需要发布
            dirty: true,
            // 事件通道
            events,
            // 未停止
            stopped: Arc::new(AtomicBool::new(false)),
            // 默认协议状态
            protocol: Protocol::default(),
            // 修订号从 0 开始
            revision: Arc::new(AtomicU64::new(0)),
            // 无配对记录
            records: BTreeMap::new(),
            // 无端点
            endpoints: BTreeMap::new(),
            // 无选中项
            selected: None,
            // 测试中不做真实发现
            discovery: Discovery::disabled(),
            // 尚无套接字
            socket: None,
            // 连接代次为 0
            generation: 0,
            // 生命周期状态
            lifecycle: Lifecycle {
                // 可见
                visible: true,
                // 默认允许
                permitted: true,
            },
            // 无配对邀请
            pairing_offer: None,
            // 无配对期限
            pairing_deadline: None,
            // 无重试时刻
            retry_at: None,
            // 重试次数为 0
            retries: 0,
            // 默认资源状态
            assets: AssetState::default(),
            // 能力集合为空
            capabilities: BTreeSet::new(),
            // 凭据服务名
            credential_service: service.into(),
            // 尚未上报设备形态
            form_factor_reported: false,
        }
    }

    /// 测试读取最近一次提交的公开投影。
    // 测试读取最近一次提交的公开投影
    pub(super) fn published_view(&mut self) -> Value {
        // 提交变更
        self.commit();
        // 锁定已发布状态
        let state = self.published.state.lock().unwrap();
        // 解析当前视图
        serde_json::from_str(&state.view).unwrap()
    }
}

/// 在 owner 线程启动前发布空投影，界面初始化无需等待凭据读取。
// 在 owner 线程启动前发布空投影，界面初始化无需等待凭据读取
pub(super) fn publish_initial(setup: &Setup) {
    // 构造空视图
    let view = empty_view(&setup.id, &setup.name);
    // 取已发布状态
    let mut state = setup
        // 已发布状态
        .published
        // 内部状态
        .state
        // 加锁
        .lock()
        // 锁中毒时仍取出内部值
        .unwrap_or_else(|p| p.into_inner());
    // 写入空视图
    state.view = view.to_string();
}

/// owner 线程入口。安全存储不可用时保留错误投影，等待用户重试或宿主恢复前台。
// owner 线程入口；安全存储不可用时保留错误投影，等待重试或宿主恢复前台
pub(super) fn run(setup: &mut Setup, rx: Receiver<Event>) {
    // 存储不可用期间到达的最新生命周期事实，在 owner 就绪后补上。
    // 存储不可用期间最新的生命周期事实，owner 就绪后补上
    let mut lifecycle: Option<HostCommand> = None;
    // 外层循环：构造 owner，失败则等待重试
    loop {
        // 已停止
        if setup.stopped.load(Ordering::SeqCst) {
            // 直接返回
            return;
        }
        // 创建所有者
        match Owner::new(setup) {
            // 构造成功：补上生命周期后进入事件循环
            Ok(mut owner) => {
                // 有等待中的生命周期命令
                if let Some(command) = lifecycle.take() {
                    // 先交给宿主处理
                    let _ = owner.host(command);
                }
                // 进入主循环
                owner.run(rx);
                // 结束
                return;
            }
            // 构造失败：发布错误投影
            Err(error) => {
                // 创建失败：构造空视图
                let mut view = empty_view(&setup.id, &setup.name);
                // 写入错误信息
                view["error"] = json!(error);
                // 状态：安全存储或连接不可用
                view["status"] = json!("安全存储或连接不可用，请解锁后重试");
                // 取已发布状态
                let mut state = setup
                    // 已发布状态
                    .published
                    // 内部状态
                    .state
                    // 加锁
                    .lock()
                    // 锁中毒时仍取出内部值
                    .unwrap_or_else(|p| p.into_inner());
                // 写入新视图
                state.view = view.to_string();
                // 释放锁
                drop(state);
            }
        }
        // 内层循环：等待重试信号
        loop {
            // 已停止
            if setup.stopped.load(Ordering::SeqCst) {
                // 直接返回
                return;
            }
            // 等待命令
            match rx.recv() {
                // 收到命令
                Ok(Event::Command(QueuedCommand {
                    // 生命周期命令
                    command: Command::Host(command @ HostCommand::Lifecycle { .. }),
                    // 其余字段忽略
                    ..
                // 收到生命周期命令
                })) => {
                    // 记下该命令
                    lifecycle = Some(command);
                    // 跳出等待
                    break;
                }
                // 收到普通命令
                Ok(Event::Command(QueuedCommand {
                    // 用户刷新或重新发现：重试
                    command: Command::Refresh | Command::Discover,
                    // 其余字段忽略
                    ..
                // 收到该命令则跳出
                })) => break,
                // 普通命令
                Ok(Event::Command(QueuedCommand {
                    // 停止或发送端断开：退出
                    command: Command::Stop,
                    // 其余字段忽略
                    ..
                }))
                // 出错则返回
                | Err(_) => return,
                // 其余事件忽略
                _ => {}
            }
        }
    }
}
