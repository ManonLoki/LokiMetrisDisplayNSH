//! Minimal WatchConnectivity bindings; every session operation stays on one worker.
use objc2::{
    AnyThread, define_class, extern_class, extern_methods, extern_protocol, msg_send,
    rc::Retained,
    runtime::{AnyObject, Bool, ProtocolObject},
};
use objc2_foundation::{
    NSArray, NSData, NSDictionary, NSError, NSObject, NSObjectProtocol, NSString, NSURL,
};
use std::path::PathBuf;

#[link(name = "WatchConnectivity", kind = "framework")]
unsafe extern "C" {}

extern_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[derive(Debug)]
    pub(super) struct WCSession;
);
extern_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[derive(Debug)]
    pub(super) struct WCSessionFileTransfer;
);
extern_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[derive(Debug)]
    struct WCSessionFile;
);

// These declarations mirror WCSession.h; no session object crosses the worker boundary.
extern_protocol!(
    #[expect(
        clippy::missing_safety_doc,
        reason = "objc2 generates an additional unsafe protocol trait with macro spans; the binding contract is documented here"
    )]
    /// WatchConnectivity's process delegate, called on its serial background queue.
    ///
    /// # Safety
    /// Implementations must use the exact WCSessionDelegate selectors and ABI,
    /// support callbacks from any thread, and not retain borrowed callback arguments.
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
        #[unsafe(method(isPaired))]
        #[unsafe(method_family = none)]
        pub(super) fn paired(&self) -> bool;
        #[unsafe(method(isWatchAppInstalled))]
        #[unsafe(method_family = none)]
        pub(super) fn installed(&self) -> bool;
        #[unsafe(method(activateSession))]
        #[unsafe(method_family = none)]
        pub(super) fn activate(&self);
        #[unsafe(method(setDelegate:))]
        #[unsafe(method_family = none)]
        fn set_delegate(&self, delegate: Option<&ProtocolObject<dyn WCSessionDelegate>>);
        #[unsafe(method(outstandingFileTransfers))]
        #[unsafe(method_family = none)]
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

    pub(super) fn context(&self, bytes: &[u8]) -> Result<(), isize> {
        let context = NSDictionary::from_slices(
            &[&*NSString::from_str("lokiCompanion")],
            &[&*NSData::with_bytes(bytes)],
        );
        let mut error: *mut NSError = std::ptr::null_mut();
        // SAFETY: the dictionary holds only a copied property-list NSData; error is
        // a valid NSError out pointer. The worker calls this only while activated.
        let success: Bool =
            unsafe { msg_send![self, updateApplicationContext: &*context, error: &mut error] };
        if success.as_bool() {
            Ok(())
        } else {
            // SAFETY: NSError is borrowed from this autorelease scope, when present.
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

fn text(dictionary: &NSDictionary<NSString, AnyObject>, key: &str, max: usize) -> Option<String> {
    let value = dictionary.objectForKey(&NSString::from_str(key))?;
    let value = value.downcast_ref::<NSString>()?;
    (value.length() <= max).then(|| value.to_string())
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "LokiCompanionSessionDelegate"]
    pub(super) struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl WCSessionDelegate for Delegate {
        #[unsafe(method(session:activationDidCompleteWithState:error:))]
        fn activated(&self, _session: &WCSession, _state: isize, _error: Option<&NSError>) {
            super::activation_finished(_error.is_none() && _state == 2);
        }
        #[unsafe(method(sessionDidBecomeInactive:))]
        fn inactive(&self, _session: &WCSession) {
            super::wake();
        }
        #[unsafe(method(sessionDidDeactivate:))]
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
        fn received(&self, _session: &WCSession, message: &NSDictionary<NSString, AnyObject>) {
            if message.count() <= 4 && text(message, "op", 32).as_deref() == Some("requestLatest") {
                super::system_changed();
            }
        }
        #[unsafe(method(session:didFinishFileTransfer:error:))]
        fn file_finished(
            &self,
            _session: &WCSession,
            transfer: &WCSessionFileTransfer,
            error: Option<&NSError>,
        ) {
            if let Some(transfer) = transfer.description() {
                super::finished(transfer, error.is_none());
            }
        }
    }
);

impl Delegate {
    pub(super) fn new() -> Retained<Self> {
        // SAFETY: NSObject's initializer; the delegate has no mutable ivars and
        // callbacks only copy public facts into the bounded Rust signal queue.
        unsafe { msg_send![Self::alloc(), init] }
    }
}
