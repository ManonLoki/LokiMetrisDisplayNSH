//! 应用自己的 UIKit 启动通知；Watch 启动不依赖 scene 或 NSH VM。
//! 前后台由界面经 NSH 的 `lifecycle()` 报告，设备形态来自 `platform.idiom`。
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol};
use objc2_ui_kit::UIApplicationDidFinishLaunchingNotification;
use std::cell::RefCell;

struct Notifications {
    // 通知接收者，保持存活
    _receiver: Retained<LaunchReceiver>,
    // 启动完成时调用的回调，只调用一次
    launch: Option<fn()>,
}

// 卸载时在 UIKit 主线程上移除观察
impl Drop for Notifications {
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
    struct LaunchReceiver;

    // SAFETY: 该类继承 NSObject,不改变其协议行为。
    // 不改变 NSObject 的协议行为
    unsafe impl NSObjectProtocol for LaunchReceiver {}

    impl LaunchReceiver {
        #[unsafe(method(applicationLaunched:))]
        fn application_launched(&self, _notification: &NSNotification) {
            launched();
        }
    }
);

/// 在主线程安装：应用启动完成后调用一次 `on_launch`。
pub(in crate::native) fn install(on_launch: fn()) {
    let mtm = MainThreadMarker::new().expect("iOS lifecycle installs on the main thread");
    NOTIFICATIONS.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_some() {
            return;
        }
        let receiver: Retained<LaunchReceiver> =
            unsafe { msg_send![LaunchReceiver::alloc(mtm), init] };
        // SAFETY: 接收者响应该选择器，并在 Notifications 销毁时注销。
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &receiver,
                sel!(applicationLaunched:),
                Some(UIApplicationDidFinishLaunchingNotification),
                None,
            );
        }
        *slot = Some(Notifications {
            _receiver: receiver,
            launch: Some(on_launch),
        });
    });
}

// 启动完成：只调用一次登记的回调
fn launched() {
    let launch = NOTIFICATIONS.with(|slot| slot.borrow_mut().as_mut()?.launch.take());
    if let Some(launch) = launch {
        launch();
    }
}
