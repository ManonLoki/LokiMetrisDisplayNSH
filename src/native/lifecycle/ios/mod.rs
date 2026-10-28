//! 应用自己的 UIKit 通知适配；Watch 启动不依赖 scene 或 NSH VM。
// 引入观察者与场景可见性
use super::{
    // 活动观察者及其集合
    ios_observers::{ActivityObserver, ActivityObservers},
    // 场景变更与场景可见性
    ios_state::{SceneChange, SceneVisibility},
};
// 引入 objc2 的类定义与消息发送
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel};
// 引入通知中心
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol};
// 引入 UIKit 的应用与场景通知
use objc2_ui_kit::{
    // 应用、场景与相关通知名称
    UIApplication,
    // 应用启动完成通知
    UIApplicationDidFinishLaunchingNotification,
    // 应用状态
    UIApplicationState,
    // 场景
    UIScene,
    // 场景激活状态
    UISceneActivationState,
    // 场景激活通知
    UISceneDidActivateNotification,
    // 场景断开通知
    UISceneDidDisconnectNotification,
    // 场景进入后台通知
    UISceneDidEnterBackgroundNotification,
    // 场景将连接通知
    UISceneWillConnectNotification,
    // 场景将失活通知
    UISceneWillDeactivateNotification,
    // 场景将进入前台通知
    UISceneWillEnterForegroundNotification,
};
// 引入内部可变性与一次性初始化
use std::{
    // 可变单元
    cell::RefCell,
    // 原子引用计数与一次性初始化
    sync::{Arc, OnceLock},
};

// 全局观察者集合
fn observers() -> &'static Arc<ActivityObservers> {
    // 观察者集合的全局单例
    static OBSERVERS: OnceLock<Arc<ActivityObservers>> = OnceLock::new();
    // 首次使用时创建
    OBSERVERS.get_or_init(|| Arc::new(ActivityObservers::default()))
}

// 已安装的通知状态
struct Notifications {
    // 通知接收者，保持存活
    _receiver: Retained<LifecycleReceiver>,
    // 启动完成时调用的回调，只调用一次
    launch: Option<fn()>,
    // 是否已收到启动通知
    launched: bool,
    // 场景可见性状态
    scenes: SceneVisibility,
}

// 卸载时在 UIKit 主线程上移除观察
impl Drop for Notifications {
    // 销毁时
    fn drop(&mut self) {
        // SAFETY: 这是由 install 注册的接收者;线程本地
        // 所有权保证它在 UIKit 主线程上被注销。
        unsafe { NSNotificationCenter::defaultCenter().removeObserver(&self._receiver) };
    }
}

// 状态放在主线程的线程局部变量中
thread_local! {
    // 线程局部的通知状态
    static NOTIFICATIONS: RefCell<Option<Notifications>> = const { RefCell::new(None) };
}

// 定义继承 NSObject 的主线程接收者类
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LokiApplicationLifecycleReceiver"]
    struct LifecycleReceiver;

    // SAFETY: 该类继承 NSObject,不改变其协议行为。
    // 不改变 NSObject 的协议行为
    unsafe impl NSObjectProtocol for LifecycleReceiver {}

    // 生命周期接收器的方法
    impl LifecycleReceiver {
        #[unsafe(method(applicationLaunched:))]
        // 应用启动完成
        fn application_launched(&self, _notification: &NSNotification) {
            // 启动完成
            launched(self.mtm());
        }

        #[unsafe(method(sceneForeground:))]
        // 场景进入前台
        fn scene_foreground(&self, notification: &NSNotification) {
            // 场景进入前台
            refresh(self.mtm(), scene_id(notification).map(SceneChange::Foreground));
        }

        #[unsafe(method(sceneBackground:))]
        // 场景进入后台
        fn scene_background(&self, notification: &NSNotification) {
            // 场景进入后台
            refresh(self.mtm(), scene_id(notification).map(SceneChange::Background));
        }

        #[unsafe(method(sceneDisconnected:))]
        // 场景断开
        fn scene_disconnected(&self, notification: &NSNotification) {
            // 场景断开
            refresh(self.mtm(), scene_id(notification).map(SceneChange::Disconnected));
        }

        #[unsafe(method(sceneChanged:))]
        // 场景激活状态变化，重新采样
        fn scene_changed(&self, _notification: &NSNotification) {
            // 应用状态变化后重新采样
            refresh(self.mtm(), None);
        }
    }
);

/// 在 runner 构造 native registry 时调用；此时只注册 Foundation 通知。
// 在 runner 构造 native registry 时调用；此时只注册 Foundation 通知
pub(in crate::native) fn install(on_launch: fn()) {
    // 必须在主线程
    let mtm = MainThreadMarker::new().expect("iOS lifecycle installs on the main thread");
    // 访问线程局部状态
    NOTIFICATIONS.with(|slot| {
        // 可变借用
        let mut slot = slot.borrow_mut();
        // 已安装则忽略
        if slot.is_some() {
            // 已安装则返回
            return;
        }
        // SAFETY: 这个主线程类使用 NSObject 的指定初始化方法。
        // 创建接收者
        let receiver: Retained<LifecycleReceiver> =
            // 创建并初始化接收器
            unsafe { msg_send![LifecycleReceiver::alloc(mtm), init] };
        // 默认通知中心
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: UIKit 的不可变通知名称,其选择器的
        // 签名接受 NSNotification。接收者与进程同寿命。
        unsafe {
            // 订阅启动完成与各类场景通知
            for (name, selector) in [
                // 通知与选择器对
                (
                    // 应用启动完成
                    UIApplicationDidFinishLaunchingNotification,
                    // 对应选择器
                    sel!(applicationLaunched:),
                ),
                // 通知与选择器对
                (
                    // 场景将进入前台
                    UISceneWillEnterForegroundNotification,
                    // 对应选择器
                    sel!(sceneForeground:),
                ),
                // 场景进入后台通知
                (
                    // 对应选择器
                    UISceneDidEnterBackgroundNotification,
                    // 选择器结束
                    sel!(sceneBackground:),
                ),
                // 场景断开通知
                (UISceneDidDisconnectNotification, sel!(sceneDisconnected:)),
                // 其余三种只触发重新采样
                (UISceneWillConnectNotification, sel!(sceneChanged:)),
                // 场景激活
                (UISceneDidActivateNotification, sel!(sceneChanged:)),
                // 场景将失活
                (UISceneWillDeactivateNotification, sel!(sceneChanged:)),
                // 逐项登记
            ] {
                // 登记通知观察
                center.addObserver_selector_name_object(&receiver, selector, Some(name), None);
            }
        }
        *slot = Some(Notifications {
            // 接收器
            _receiver: receiver,
            // 启动回调
            launch: Some(on_launch),
            // 尚未启动
            launched: false,
            // 场景可见性初始为空
            scenes: SceneVisibility::default(),
        });
    });
}

// 读取当前前台状态
pub(super) fn foreground() -> Option<bool> {
    // 当前是否前台
    observers().foreground()
}

/// 初始事实与后续通知都由主队列交付；关闭 VM 后排队的初始回调不再触发。
// 初始事实与后续通知都由主队列交付；关闭 VM 后排队的初始回调不再触发
pub(in crate::native) fn observe(
    // 回调
    callback: impl Fn(bool) + Send + Sync + 'static,
    // 返回观察者
) -> ActivityObserver {
    // 登记回调
    let observer = observers().observe(callback);
    // 观察者标识
    let id = observer.id();
    // 在主线程异步执行
    dispatch2::DispatchQueue::main().exec_async(move || {
        // 投递当前状态
        observers().deliver(id);
    });
    // 返回观察者
    observer
}

// 处理启动完成：只触发一次启动回调
fn launched(mtm: MainThreadMarker) {
    // 取出启动回调
    let launch = NOTIFICATIONS.with(|slot| {
        // 可变借用
        let mut slot = slot.borrow_mut();
        // 取出通知状态
        let notifications = slot.as_mut()?;
        // 已经启动过
        if notifications.launched {
            // 不再重复
            return None;
        }
        // 标记已启动
        notifications.launched = true;
        // 取走启动回调
        notifications.launch.take()
    });
    // 先采样一次可见性
    refresh(mtm, None);
    // 即使后台投递没有创建场景,UIKit 也会发出这个进程通知。
    // 没有 NSH 界面或 JS 回调会启动 WatchConnectivity。
    // UIKit 即使在后台投递且没有创建场景时也会发这个进程通知；这里不需要 NSH 界面或 JS 回调就能启动 WatchConnectivity
    if let Some(launch) = launch {
        // 执行启动回调
        launch();
    }
}

// 取通知对象对应的场景指针作为标识
fn scene_id(notification: &NSNotification) -> Option<usize> {
    // 通知携带的对象
    let object = notification.object()?;
    // 转为场景引用
    let scene = object.downcast_ref::<UIScene>()?;
    // 以指针值作为场景标识
    Some(scene as *const UIScene as usize)
}

// 重新采样可见性并发布
fn refresh(mtm: MainThreadMarker, change: Option<SceneChange>) {
    // 启动完成前不采样
    let visible = NOTIFICATIONS.with(|slot| {
        // 可变借用
        let mut slot = slot.borrow_mut();
        // 取出通知状态
        let notifications = slot
            // 取可变引用
            .as_mut()
            // 须已经启动
            .filter(|notifications| notifications.launched)?;
        // 共享应用对象
        let application = UIApplication::sharedApplication(mtm);
        // 已连接的场景
        let scenes = application.connectedScenes();
        // 前台激活或前台非激活都算可见
        let scenes = scenes.iter().map(|scene| {
            // 场景激活状态
            let state = scene.activationState();
            // 前台活跃才算可见
            let visible = state == UISceneActivationState::ForegroundActive
                // 或者处于前台未激活
                || state == UISceneActivationState::ForegroundInactive;
            // 场景标识与可见性
            (Retained::as_ptr(&scene) as usize, visible)
        });
        // 采样得到整体可见性
        Some(notifications.scenes.sample(
            // 应用不在后台
            application.applicationState() != UIApplicationState::Background,
            // 场景集合
            scenes,
            // 变更
            change,
        ))
    });
    // 可见性有变化
    if let Some(visible) = visible {
        // 通知观察者
        observers().publish(visible);
    }
}
