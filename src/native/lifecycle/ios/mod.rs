//! 应用自己的 UIKit 启动通知与设备形态采样；Watch 启动不依赖 scene 或 NSH VM。
//! 前后台由界面经 NSH 的 `lifecycle()` 报告，这里不再观察 scene。
// 引入 objc2 的类定义与消息发送
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel};
// 引入通知中心
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol};
// 引入 UIKit 的启动通知、设备类型
use objc2_ui_kit::{UIApplicationDidFinishLaunchingNotification, UIDevice, UIUserInterfaceIdiom};
// 引入内部可变性与一次性初始化
use std::{cell::RefCell, sync::OnceLock};

/// UIKit启动时采样并冻结安装设备形态；JS线程只读此值，不跨线程调用UIKit。
static DEVICE_FORM_FACTOR: OnceLock<Option<&'static str>> = OnceLock::new();

/// 返回实际安装设备报告的手机/平板形态；未知或尚未初始化保持None。
pub(in crate::native) fn form_factor() -> Option<&'static str> {
    DEVICE_FORM_FACTOR.get().copied().flatten()
}

// 已安装的通知状态
struct Notifications {
    // 通知接收者，保持存活
    _receiver: Retained<LaunchReceiver>,
    // 启动完成时调用的回调，只调用一次
    launch: Option<fn()>,
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
    struct LaunchReceiver;

    // SAFETY: 该类继承 NSObject,不改变其协议行为。
    // 不改变 NSObject 的协议行为
    unsafe impl NSObjectProtocol for LaunchReceiver {}

    // 启动接收器的方法
    impl LaunchReceiver {
        #[unsafe(method(applicationLaunched:))]
        // 应用启动完成
        fn application_launched(&self, _notification: &NSNotification) {
            // 启动完成
            launched();
        }
    }
);

/// 在主线程安装：采样设备形态，并在应用启动完成后调用一次 `on_launch`。
pub(in crate::native) fn install(on_launch: fn()) {
    let mtm = MainThreadMarker::new().expect("iOS lifecycle installs on the main thread");
    DEVICE_FORM_FACTOR.get_or_init(|| match UIDevice::currentDevice(mtm).userInterfaceIdiom() {
        UIUserInterfaceIdiom::Phone => Some("phone"),
        UIUserInterfaceIdiom::Pad => Some("tablet"),
        _ => None,
    });
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
