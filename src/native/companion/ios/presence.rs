//! 工作线程根据已发布的快照和系统会话状态发起探测。

use super::{Actor, default_root, read_manifest};
use crate::native::companion::presence::ExpectedSnapshot;
use std::time::Instant;

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
