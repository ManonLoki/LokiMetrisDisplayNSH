//! 最小的 WatchConnectivity 绑定;每个会话操作都留在同一个工作线程上。
use objc2::{
    // Objective-C 类定义宏
    AnyThread,
    define_class,
    extern_class,
    extern_methods,
    extern_protocol,
    msg_send,
    rc::Retained,
    // 任意对象、布尔与协议对象
    runtime::{AnyObject, Bool, ProtocolObject},
};
use objc2_foundation::{
    // Foundation 类型
    NSArray,
    NSData,
    NSDictionary,
    NSError,
    NSObject,
    NSObjectProtocol,
    NSString,
    NSURL,
};
use std::path::PathBuf;

// 链接 WatchConnectivity 框架
#[link(name = "WatchConnectivity", kind = "framework")]
unsafe extern "C" {}

// 声明 WCSession、WCSessionFileTransfer、WCSessionFile 三个外部类
extern_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[derive(Debug)]
    pub(super) struct WCSession;
);
// 声明 WCSession 类
extern_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[derive(Debug)]
    pub(super) struct WCSessionFileTransfer;
);
// 声明 WCSessionFile 类
extern_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[derive(Debug)]
    struct WCSessionFile;
);

// 这些声明镜像 WCSession.h;没有会话对象会越过工作线程边界。
// 对应 WCSession.h 的声明；会话对象不会跨越工作线程边界
extern_protocol!(
    #[expect(
        clippy::missing_safety_doc,
        // 说明：宏会生成额外的不安全协议特征
        reason = "objc2 generates an additional unsafe protocol trait with macro spans; the binding contract is documented here"
    )]
    /// WatchConnectivity 的进程委托,在其串行后台队列上被调用。
    ///
    /// # Safety
    /// 实现必须使用精确的 WCSessionDelegate 选择器与 ABI,
    /// 支持来自任意线程的回调,且不得保留被借用的回调参数。
    // WatchConnectivity 的进程级代理，在其串行后台队列上回调
    unsafe trait WCSessionDelegate: NSObjectProtocol {
        #[unsafe(method(session:activationDidCompleteWithState:error:))]
        fn activated(&self, session: &WCSession, state: isize, error: Option<&NSError>);
        #[unsafe(method(sessionDidBecomeInactive:))]
        fn inactive(&self, session: &WCSession);
        #[unsafe(method(sessionDidDeactivate:))]
        fn deactivated(&self, session: &WCSession);
        #[optional]
        #[unsafe(method(sessionWatchStateDidChange:))]
        fn watch_changed(&self, session: &WCSession);
        #[optional]
        #[unsafe(method(sessionReachabilityDidChange:))]
        fn reachability_changed(&self, session: &WCSession);
        #[optional]
        #[unsafe(method(session:didReceiveMessage:))]
        fn received(&self, session: &WCSession, message: &NSDictionary<NSString, AnyObject>);
        #[optional]
        #[unsafe(method(session:didReceiveMessageData:))]
        // Watch前台撤销使用独立的小JSON，不影响背景文件传输。
        fn received_data(&self, session: &WCSession, data: &NSData);
        #[optional]
        #[unsafe(method(session:didFinishFileTransfer:error:))]
        fn file_finished(
            &self,
            session: &WCSession,
            transfer: &WCSessionFileTransfer,
            error: Option<&NSError>,
        );
    }
);

impl WCSession {
    // 声明所需的 Objective-C 方法
    extern_methods!(
        #[unsafe(method(isSupported))]
        #[unsafe(method_family = none)]
        pub(super) fn is_supported() -> bool;
        #[unsafe(method(defaultSession))]
        #[unsafe(method_family = none)]
        pub(super) fn shared() -> Retained<Self>;
        #[unsafe(method(activationState))]
        #[unsafe(method_family = none)]
        pub(super) fn activation_state(&self) -> isize;
        #[unsafe(method(isReachable))]
        #[unsafe(method_family = none)]
        // 仅为发送前提，不能用作实际Watch在线证明。
        pub(super) fn reachable(&self) -> bool;
        #[unsafe(method(isPaired))]
        #[unsafe(method_family = none)]
        pub(super) fn paired(&self) -> bool;
        #[unsafe(method(isWatchAppInstalled))]
        #[unsafe(method_family = none)]
        // 手表应用是否已安装
        pub(super) fn installed(&self) -> bool;
        #[unsafe(method(activateSession))]
        #[unsafe(method_family = none)]
        pub(super) fn activate(&self);
        #[unsafe(method(setDelegate:))]
        #[unsafe(method_family = none)]
        fn set_delegate(&self, delegate: Option<&ProtocolObject<dyn WCSessionDelegate>>);
        #[unsafe(method(outstandingFileTransfers))]
        #[unsafe(method_family = none)]
        // 尚未完成的文件传输
        pub(super) fn outstanding(&self) -> Retained<NSArray<WCSessionFileTransfer>>;
        #[unsafe(method(transferFile:metadata:))]
        #[unsafe(method_family = none)]
        pub(super) fn transfer_file(
            &self,
            file: &NSURL,
            metadata: Option<&NSDictionary<NSString, NSString>>,
        ) -> Retained<WCSessionFileTransfer>;
    );

    pub(super) fn attach(&self, delegate: &Delegate) {
        self.set_delegate(Some(ProtocolObject::from_ref(delegate)));
    }

    // 更新应用上下文：字典里只有一份复制的属性列表数据
    pub(super) fn context(&self, bytes: &[u8]) -> Result<(), isize> {
        let context = NSDictionary::from_slices(
            &[&*NSString::from_str("lokiCompanion")],
            &[&*NSData::with_bytes(bytes)],
        );
        let mut error: *mut NSError = std::ptr::null_mut();
        // SAFETY: 字典只持有复制出来的属性列表 NSData;error 是
        // 有效的 NSError 输出指针。工作线程仅在已激活时调用它。
        // 仅在会话已激活时由工作线程调用
        let success: Bool =
            unsafe { msg_send![self, updateApplicationContext: &*context, error: &mut error] };
        if success.as_bool() {
            Ok(())
        // 否则分支
        } else {
            // SAFETY: 若存在,NSError 是从这个 autorelease 作用域借用的。
            Err(unsafe { error.as_ref() }.map_or(0, NSError::code))
        }
    }
}

impl WCSessionFileTransfer {
    extern_methods!(
        #[unsafe(method(cancel))]
        #[unsafe(method_family = none)]
        pub(super) fn cancel(&self);
        #[unsafe(method(file))]
        #[unsafe(method_family = none)]
        fn file(&self) -> Retained<WCSessionFile>;
    );

    // 解析传输的元数据：必须是资源类型，字段长度有界
    pub(super) fn description(&self) -> Option<super::Transfer> {
        let file = self.file();
        let metadata = file.metadata()?;
        if text(&metadata, "kind", 16).as_deref() != Some("asset") {
            return None;
        }
        Some(super::Transfer {
            stream: text(&metadata, "streamId", 36)?,
            revision: text(&metadata, "revision", 20)?.parse().ok()?,
            hash: text(&metadata, "hash", 64)?,
            path: PathBuf::from(file.url().path()?.to_string()),
        })
    }
}

impl WCSessionFile {
    extern_methods!(
        #[unsafe(method(metadata))]
        #[unsafe(method_family = none)]
        fn metadata(&self) -> Option<Retained<NSDictionary<NSString, AnyObject>>>;
        #[unsafe(method(fileURL))]
        #[unsafe(method_family = none)]
        fn url(&self) -> Retained<NSURL>;
    );
}

// 读取元数据中的字符串，长度不得超过上限
fn text(dictionary: &NSDictionary<NSString, AnyObject>, key: &str, max: usize) -> Option<String> {
    let value = dictionary.objectForKey(&NSString::from_str(key))?;
    let value = value.downcast_ref::<NSString>()?;
    // 长度不超过上限才返回
    (value.length() <= max).then(|| value.to_string())
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "LokiCompanionSessionDelegate"]
    pub(super) struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl WCSessionDelegate for Delegate {
        #[unsafe(method(session:activationDidCompleteWithState:error:))]
        // 激活完成：无错误且状态为 2（已激活）才算成功
        fn activated(&self, _session: &WCSession, _state: isize, _error: Option<&NSError>) {
            // 激活完成：无错误且状态为已激活
            super::activation_finished(_error.is_none() && _state == 2);
        }
        #[unsafe(method(sessionDidBecomeInactive:))]
        // 不活跃：唤醒工作线程
        fn inactive(&self, _session: &WCSession) {
            super::wake();
        }
        #[unsafe(method(sessionDidDeactivate:))]
        // 已停用：通知系统状态变化
        fn deactivated(&self, _session: &WCSession) {
            super::system_changed();
        }
        #[unsafe(method(sessionWatchStateDidChange:))]
        fn watch_changed(&self, _session: &WCSession) {
            super::system_changed();
        }
        #[unsafe(method(sessionReachabilityDidChange:))]
        fn reachability_changed(&self, _session: &WCSession) {
            super::system_changed();
        }
        #[unsafe(method(session:didReceiveMessage:))]
        // 手表请求最新数据：消息很小且 op 为 requestLatest
        fn received(&self, _session: &WCSession, message: &NSDictionary<NSString, AnyObject>) {
            // 收到请求最新数据的消息
            if message.count() <= 4 && text(message, "op", 32).as_deref() == Some("requestLatest") {
                super::system_changed();
            }
        }
        #[unsafe(method(session:didReceiveMessageData:))]
        // 回调内有界复制，worker再验证身份/前台代次，不由WC线程修改连接状态。
        fn received_data(&self, _session: &WCSession, data: &NSData) {
            super::presence::revoked(data);
        }
        #[unsafe(method(session:didFinishFileTransfer:error:))]
        // 文件传输结束：记录成功与否
        fn file_finished(
            &self,
            _session: &WCSession,
            transfer: &WCSessionFileTransfer,
            error: Option<&NSError>,
        ) {
            if let Some(transfer) = transfer.description() {
                // 记录传输结束及其结果
                super::finished(transfer, error.is_none());
            }
        }
    }
);

impl Delegate {
    // 创建代理：没有可变实例变量，回调只把公开事实复制进有界的 Rust 信号队列
    pub(super) fn new() -> Retained<Self> {
        // SAFETY: NSObject 的初始化方法;委托没有可变实例变量,
        // 回调只会把公开事实复制进有界的 Rust 信号队列。
        unsafe { msg_send![Self::alloc(), init] }
    }
}
