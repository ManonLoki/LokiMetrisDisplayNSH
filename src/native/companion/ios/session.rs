//! 最小的 WatchConnectivity 绑定;每个会话操作都留在同一个工作线程上。
// 引入 objc2 的类、协议与消息发送
use objc2::{
    // Objective-C 类定义宏
    AnyThread,
    // 类定义宏
    define_class,
    // 外部类声明
    extern_class,
    // 外部方法声明
    extern_methods,
    // 外部协议声明
    extern_protocol,
    // 发送消息
    msg_send,
    // 引用计数对象
    rc::Retained,
    // 任意对象、布尔与协议对象
    runtime::{AnyObject, Bool, ProtocolObject},
};
// 引入 Foundation 类型
use objc2_foundation::{
    // Foundation 类型
    NSArray,
    // 数据
    NSData,
    // 字典
    NSDictionary,
    // 错误
    NSError,
    // 基础对象
    NSObject,
    // 对象协议
    NSObjectProtocol,
    // 字符串
    NSString,
    // 地址
    NSURL,
};
// 引入路径
use std::path::PathBuf;

// 链接 WatchConnectivity 框架
#[link(name = "WatchConnectivity", kind = "framework")]
unsafe extern "C" {}

// 声明 WCSession、WCSessionFileTransfer、WCSessionFile 三个外部类
extern_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[derive(Debug)]
    // 会话
    pub(super) struct WCSession;
);
// 声明 WCSession 类
extern_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[derive(Debug)]
    // 文件传输
    pub(super) struct WCSessionFileTransfer;
);
// 声明 WCSessionFile 类
extern_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[derive(Debug)]
    // 传输的文件
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
        // 激活完成
        fn activated(&self, session: &WCSession, state: isize, error: Option<&NSError>);
        #[unsafe(method(sessionDidBecomeInactive:))]
        // 会话变为不活跃
        fn inactive(&self, session: &WCSession);
        #[unsafe(method(sessionDidDeactivate:))]
        // 会话已停用
        fn deactivated(&self, session: &WCSession);
        #[optional]
        #[unsafe(method(sessionWatchStateDidChange:))]
        // 手表状态变化
        fn watch_changed(&self, session: &WCSession);
        #[optional]
        #[unsafe(method(sessionReachabilityDidChange:))]
        // 可达性变化
        fn reachability_changed(&self, session: &WCSession);
        #[optional]
        #[unsafe(method(session:didReceiveMessage:))]
        // 收到消息
        fn received(&self, session: &WCSession, message: &NSDictionary<NSString, AnyObject>);
        #[optional]
        #[unsafe(method(session:didFinishFileTransfer:error:))]
        // 文件传输结束
        fn file_finished(
            // 自身
            &self,
            // 会话
            session: &WCSession,
            // 文件传输
            transfer: &WCSessionFileTransfer,
            // 错误
            error: Option<&NSError>,
        );
    }
);

// 会话的方法
impl WCSession {
    // 声明所需的 Objective-C 方法
    extern_methods!(
        #[unsafe(method(isSupported))]
        #[unsafe(method_family = none)]
        // 设备是否支持
        pub(super) fn is_supported() -> bool;
        #[unsafe(method(defaultSession))]
        #[unsafe(method_family = none)]
        // 默认会话
        pub(super) fn shared() -> Retained<Self>;
        #[unsafe(method(activationState))]
        #[unsafe(method_family = none)]
        // 激活状态
        pub(super) fn activation_state(&self) -> isize;
        #[unsafe(method(isPaired))]
        #[unsafe(method_family = none)]
        // 是否已配对手表
        pub(super) fn paired(&self) -> bool;
        #[unsafe(method(isWatchAppInstalled))]
        #[unsafe(method_family = none)]
        // 手表应用是否已安装
        pub(super) fn installed(&self) -> bool;
        #[unsafe(method(activateSession))]
        #[unsafe(method_family = none)]
        // 激活会话
        pub(super) fn activate(&self);
        #[unsafe(method(setDelegate:))]
        #[unsafe(method_family = none)]
        // 设置代理
        fn set_delegate(&self, delegate: Option<&ProtocolObject<dyn WCSessionDelegate>>);
        #[unsafe(method(outstandingFileTransfers))]
        #[unsafe(method_family = none)]
        // 尚未完成的文件传输
        pub(super) fn outstanding(&self) -> Retained<NSArray<WCSessionFileTransfer>>;
        #[unsafe(method(transferFile:metadata:))]
        #[unsafe(method_family = none)]
        // 发起文件传输
        pub(super) fn transfer_file(
            // 自身
            &self,
            // 文件
            file: &NSURL,
            // 元数据
            metadata: Option<&NSDictionary<NSString, NSString>>,
            // 返回文件传输
        ) -> Retained<WCSessionFileTransfer>;
    );

    // 挂接代理
    pub(super) fn attach(&self, delegate: &Delegate) {
        // 设置委托
        self.set_delegate(Some(ProtocolObject::from_ref(delegate)));
    }

    // 更新应用上下文：字典里只有一份复制的属性列表数据
    pub(super) fn context(&self, bytes: &[u8]) -> Result<(), isize> {
        // 组装上下文字典
        let context = NSDictionary::from_slices(
            // 键：配套上下文
            &[&*NSString::from_str("lokiCompanion")],
            // 值：数据
            &[&*NSData::with_bytes(bytes)],
        );
        // 接收错误的指针
        let mut error: *mut NSError = std::ptr::null_mut();
        // SAFETY: 字典只持有复制出来的属性列表 NSData;error 是
        // 有效的 NSError 输出指针。工作线程仅在已激活时调用它。
        // 仅在会话已激活时由工作线程调用
        let success: Bool =
            // 更新应用上下文
            unsafe { msg_send![self, updateApplicationContext: &*context, error: &mut error] };
        // 更新成功
        if success.as_bool() {
            // 返回成功
            Ok(())
        // 否则分支
        } else {
            // SAFETY: 若存在,NSError 是从这个 autorelease 作用域借用的。
            // 失败返回错误码
            Err(unsafe { error.as_ref() }.map_or(0, NSError::code))
        }
    }
}

// 文件传输的方法
impl WCSessionFileTransfer {
    // 声明外部方法
    extern_methods!(
        #[unsafe(method(cancel))]
        #[unsafe(method_family = none)]
        // 取消传输
        pub(super) fn cancel(&self);
        #[unsafe(method(file))]
        #[unsafe(method_family = none)]
        // 取得传输的文件
        fn file(&self) -> Retained<WCSessionFile>;
    );

    // 解析传输的元数据：必须是资源类型，字段长度有界
    pub(super) fn description(&self) -> Option<super::Transfer> {
        // 传输的文件
        let file = self.file();
        // 文件元数据
        let metadata = file.metadata()?;
        // 种类不是资源
        if text(&metadata, "kind", 16).as_deref() != Some("asset") {
            // 返回 None
            return None;
        }
        // 构造传输
        Some(super::Transfer {
            // 所属流
            stream: text(&metadata, "streamId", 36)?,
            // 修订号
            revision: text(&metadata, "revision", 20)?.parse().ok()?,
            // 哈希
            hash: text(&metadata, "hash", 64)?,
            // 文件路径
            path: PathBuf::from(file.url().path()?.to_string()),
        })
    }
}

// 文件的方法
impl WCSessionFile {
    // 声明外部方法
    extern_methods!(
        #[unsafe(method(metadata))]
        #[unsafe(method_family = none)]
        // 元数据
        fn metadata(&self) -> Option<Retained<NSDictionary<NSString, AnyObject>>>;
        #[unsafe(method(fileURL))]
        #[unsafe(method_family = none)]
        // 文件地址
        fn url(&self) -> Retained<NSURL>;
    );
}

// 读取元数据中的字符串，长度不得超过上限
fn text(dictionary: &NSDictionary<NSString, AnyObject>, key: &str, max: usize) -> Option<String> {
    // 按键取值
    let value = dictionary.objectForKey(&NSString::from_str(key))?;
    // 须为字符串
    let value = value.downcast_ref::<NSString>()?;
    // 长度不超过上限才返回
    (value.length() <= max).then(|| value.to_string())
}

// 定义代理类
define_class!(
    #[unsafe(super(NSObject))]
    #[name = "LokiCompanionSessionDelegate"]
    // 代理类
    pub(super) struct Delegate;

    // 委托遵循对象协议
    unsafe impl NSObjectProtocol for Delegate {}

    // 委托实现会话协议
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
            // 唤醒执行者
            super::wake();
        }
        #[unsafe(method(sessionDidDeactivate:))]
        // 已停用：通知系统状态变化
        fn deactivated(&self, _session: &WCSession) {
            // 通知系统状态变化
            super::system_changed();
        }
        #[unsafe(method(sessionWatchStateDidChange:))]
        // 手表状态变化
        fn watch_changed(&self, _session: &WCSession) {
            // 通知系统状态变化
            super::system_changed();
        }
        #[unsafe(method(sessionReachabilityDidChange:))]
        // 可达性变化
        fn reachability_changed(&self, _session: &WCSession) {
            // 通知系统状态变化
            super::system_changed();
        }
        #[unsafe(method(session:didReceiveMessage:))]
        // 手表请求最新数据：消息很小且 op 为 requestLatest
        fn received(&self, _session: &WCSession, message: &NSDictionary<NSString, AnyObject>) {
            // 收到请求最新数据的消息
            if message.count() <= 4 && text(message, "op", 32).as_deref() == Some("requestLatest") {
                // 通知系统状态变化
                super::system_changed();
            }
        }
        #[unsafe(method(session:didFinishFileTransfer:error:))]
        // 文件传输结束：记录成功与否
        fn file_finished(
            // 自身
            &self,
            // 会话
            _session: &WCSession,
            // 文件传输
            transfer: &WCSessionFileTransfer,
            // 错误
            error: Option<&NSError>,
            // 方法体
        ) {
            // 取传输描述
            if let Some(transfer) = transfer.description() {
                // 记录传输结束及其结果
                super::finished(transfer, error.is_none());
            }
        }
    }
);

// 委托的方法
impl Delegate {
    // 创建代理：没有可变实例变量，回调只把公开事实复制进有界的 Rust 信号队列
    pub(super) fn new() -> Retained<Self> {
        // SAFETY: NSObject 的初始化方法;委托没有可变实例变量,
        // 回调只会把公开事实复制进有界的 Rust 信号队列。
        unsafe { msg_send![Self::alloc(), init] }
    }
}
