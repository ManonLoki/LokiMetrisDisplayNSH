//! WC回调仅复制有界公开数据到既有worker，序号与wire转换由纯Controller执行。

use super::{Actor, SIGNALS, WCSession, default_root, read_manifest, wake};
use crate::native::companion::presence::{Command, ExpectedSnapshot};
use objc2_foundation::NSData;
use std::time::Instant;

/// 最新LAN意图合并为一项，不建立第二个WC worker或周期轮询。
pub(in crate::native) fn command(command: Command) {
    SIGNALS.lock().unwrap_or_else(|p| p.into_inner()).presence = Some(command);
    wake();
}

/// 成功/失败两个原生终态共用入口；data在回调内复制后才离开Foundation借用作用域。
pub(super) fn completed(challenge: String, data: Option<&NSData>) {
    let bytes = data
        .filter(|data| data.len() <= 4096)
        .map(|data| data.to_vec());
    let mut signals = SIGNALS.lock().unwrap_or_else(|p| p.into_inner());
    if signals.presence_completed.len() < 4 {
        signals.presence_completed.push((challenge, bytes));
    }
    drop(signals);
    wake();
}

/// Watch前台撤销是独立事实；不会把requestLatest或背景文件终态转换成在线。
pub(super) fn revoked(data: &NSData) {
    if data.len() > 4096 {
        return;
    }
    let mut signals = SIGNALS.lock().unwrap_or_else(|p| p.into_inner());
    if signals.presence_revoked.len() < 16 {
        signals.presence_revoked.push(data.to_vec());
    } else {
        signals.presence_revoked_overflow = true;
    }
    drop(signals);
    wake();
}

impl Actor {
    /// 仍在原Actor线程使用同一个WCSession；OS资格只可阻止发送，不能证明Watch在线。
    pub(super) fn flush_presence(&mut self) {
        let now = Instant::now();
        self.presence.expire(now);
        if self.session.activation_state() != 2
            || !self.session.paired()
            || !self.session.installed()
            || !self.session.reachable()
        {
            self.presence.unavailable(now);
            return;
        }
        let Some((root, source_id)) = self
            .presence
            .target()
            .map(|(root, source)| (root.to_path_buf(), source.to_owned()))
        else {
            return;
        };
        if default_root().as_ref() != Some(&root) {
            self.presence.unavailable(now);
            return;
        }
        let Ok(manifest) = read_manifest(&root) else {
            return;
        };
        if manifest.source_id.as_ref() != Some(&source_id) || manifest.snapshot.is_none() {
            self.presence.unavailable(now);
            return;
        }
        let expected = ExpectedSnapshot {
            stream_id: manifest.stream_id,
            revision: manifest.revision,
            source_id,
        };
        if let Some((challenge, bytes)) = self.presence.start(expected, now) {
            self.session.probe(challenge, &bytes);
        }
    }
}

impl WCSession {
    /// WCSession复制escaping block；捕获只含Send的String，回调中不借Actor或连接owner。
    pub(super) fn probe(&self, challenge: String, bytes: &[u8]) {
        let success_challenge = challenge.clone();
        let success = block2::RcBlock::new(move |data: *mut NSData| {
            // SAFETY: WCSession成功回调参数在此次调用期间由系统持有，入口立即复制有界字节。
            completed(success_challenge.clone(), unsafe { data.as_ref() });
        });
        let failure = block2::RcBlock::new(move |_error: *mut objc2_foundation::NSError| {
            completed(challenge.clone(), None);
        });
        let data = NSData::with_bytes(bytes);
        // SAFETY: 选择器、NSData与两个block签名对应WCSession.h；系统复制block供异步终态使用。
        let _: () = unsafe {
            objc2::msg_send![self, sendMessageData: &*data, replyHandler: &*success, errorHandler: &*failure]
        };
    }
}
