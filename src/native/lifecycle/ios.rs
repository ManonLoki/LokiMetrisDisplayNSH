//! 应用自己的 UIKit 通知适配；Watch 启动不依赖 scene 或 NSH VM。
use super::{
    ios_observers::{ActivityObserver, ActivityObservers},
    ios_state::{SceneChange, SceneVisibility},
};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol};
use objc2_ui_kit::{
    UIApplication, UIApplicationDidFinishLaunchingNotification, UIApplicationState, UIScene,
    UISceneActivationState, UISceneDidActivateNotification, UISceneDidDisconnectNotification,
    UISceneDidEnterBackgroundNotification, UISceneWillConnectNotification,
    UISceneWillDeactivateNotification, UISceneWillEnterForegroundNotification,
};
use std::{
    cell::RefCell,
    sync::{Arc, OnceLock},
};

fn observers() -> &'static Arc<ActivityObservers> {
    static OBSERVERS: OnceLock<Arc<ActivityObservers>> = OnceLock::new();
    OBSERVERS.get_or_init(|| Arc::new(ActivityObservers::default()))
}

struct Notifications {
    _receiver: Retained<LifecycleReceiver>,
    launch: Option<fn()>,
    launched: bool,
    scenes: SceneVisibility,
}

impl Drop for Notifications {
    fn drop(&mut self) {
        // SAFETY: this is the receiver registered by install; thread-local
        // ownership ensures it is unregistered on the UIKit main thread.
        unsafe { NSNotificationCenter::defaultCenter().removeObserver(&self._receiver) };
    }
}

thread_local! {
    static NOTIFICATIONS: RefCell<Option<Notifications>> = const { RefCell::new(None) };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "LokiApplicationLifecycleReceiver"]
    struct LifecycleReceiver;

    // SAFETY: the class inherits NSObject without changing its protocol behavior.
    unsafe impl NSObjectProtocol for LifecycleReceiver {}

    impl LifecycleReceiver {
        #[unsafe(method(applicationLaunched:))]
        fn application_launched(&self, _notification: &NSNotification) {
            launched(self.mtm());
        }

        #[unsafe(method(sceneForeground:))]
        fn scene_foreground(&self, notification: &NSNotification) {
            refresh(self.mtm(), scene_id(notification).map(SceneChange::Foreground));
        }

        #[unsafe(method(sceneBackground:))]
        fn scene_background(&self, notification: &NSNotification) {
            refresh(self.mtm(), scene_id(notification).map(SceneChange::Background));
        }

        #[unsafe(method(sceneDisconnected:))]
        fn scene_disconnected(&self, notification: &NSNotification) {
            refresh(self.mtm(), scene_id(notification).map(SceneChange::Disconnected));
        }

        #[unsafe(method(sceneChanged:))]
        fn scene_changed(&self, _notification: &NSNotification) {
            refresh(self.mtm(), None);
        }
    }
);

/// 在 runner 构造 native registry 时调用；此时只注册 Foundation 通知。
pub(in crate::native) fn install(on_launch: fn()) {
    let mtm = MainThreadMarker::new().expect("iOS lifecycle installs on the main thread");
    NOTIFICATIONS.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_some() {
            return;
        }
        // SAFETY: NSObject's designated initializer for this main-thread class.
        let receiver: Retained<LifecycleReceiver> =
            unsafe { msg_send![LifecycleReceiver::alloc(mtm), init] };
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: UIKit's immutable notification names, with selectors whose
        // signatures accept NSNotification. The receiver lives for the process.
        unsafe {
            for (name, selector) in [
                (
                    UIApplicationDidFinishLaunchingNotification,
                    sel!(applicationLaunched:),
                ),
                (
                    UISceneWillEnterForegroundNotification,
                    sel!(sceneForeground:),
                ),
                (
                    UISceneDidEnterBackgroundNotification,
                    sel!(sceneBackground:),
                ),
                (UISceneDidDisconnectNotification, sel!(sceneDisconnected:)),
                (UISceneWillConnectNotification, sel!(sceneChanged:)),
                (UISceneDidActivateNotification, sel!(sceneChanged:)),
                (UISceneWillDeactivateNotification, sel!(sceneChanged:)),
            ] {
                center.addObserver_selector_name_object(&receiver, selector, Some(name), None);
            }
        }
        *slot = Some(Notifications {
            _receiver: receiver,
            launch: Some(on_launch),
            launched: false,
            scenes: SceneVisibility::default(),
        });
    });
}

pub(super) fn foreground() -> Option<bool> {
    observers().foreground()
}

/// 初始事实与后续通知都由主队列交付；关闭 VM 后排队的初始回调不再触发。
pub(in crate::native) fn observe(
    callback: impl Fn(bool) + Send + Sync + 'static,
) -> ActivityObserver {
    let observer = observers().observe(callback);
    let id = observer.id();
    dispatch2::DispatchQueue::main().exec_async(move || {
        observers().deliver(id);
    });
    observer
}

fn launched(mtm: MainThreadMarker) {
    let launch = NOTIFICATIONS.with(|slot| {
        let mut slot = slot.borrow_mut();
        let notifications = slot.as_mut()?;
        if notifications.launched {
            return None;
        }
        notifications.launched = true;
        notifications.launch.take()
    });
    refresh(mtm, None);
    // UIKit posts this process notification even when background delivery
    // creates no scene. No NSH surface or JS callback starts WatchConnectivity.
    if let Some(launch) = launch {
        launch();
    }
}

fn scene_id(notification: &NSNotification) -> Option<usize> {
    let object = notification.object()?;
    let scene = object.downcast_ref::<UIScene>()?;
    Some(scene as *const UIScene as usize)
}

fn refresh(mtm: MainThreadMarker, change: Option<SceneChange>) {
    let visible = NOTIFICATIONS.with(|slot| {
        let mut slot = slot.borrow_mut();
        let notifications = slot
            .as_mut()
            .filter(|notifications| notifications.launched)?;
        let application = UIApplication::sharedApplication(mtm);
        let scenes = application.connectedScenes();
        let scenes = scenes.iter().map(|scene| {
            let state = scene.activationState();
            let visible = state == UISceneActivationState::ForegroundActive
                || state == UISceneActivationState::ForegroundInactive;
            (Retained::as_ptr(&scene) as usize, visible)
        });
        Some(notifications.scenes.sample(
            application.applicationState() != UIApplicationState::Background,
            scenes,
            change,
        ))
    });
    if let Some(visible) = visible {
        observers().publish(visible);
    }
}
