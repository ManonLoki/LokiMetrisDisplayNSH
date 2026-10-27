//! 桌面直接持有共享库的线性握手对象；这里只决定本机保存与用户取消的先后。
use loki_metris_pairing::{
    PairingAck, PairingConfirmation, PairingHello, PairingMaterial, PairingReply,
    PendingReceiverCommit, ReceiverSession,
    session::{
        DeviceSession, EncryptedPacket, SecureChannel, SessionConfirmation, SessionHello,
        SessionReply,
    },
    window::{PairingAttempt, PairingWindow},
};
use std::time::Instant;

pub(super) const AUTH_ERROR: &str = "配对或安全认证失败，请重试。";

enum SessionState {
    Disconnected,
    Pairing(PairingAttempt, ReceiverSession),
    Resuming(DeviceSession),
    Connected(SecureChannel),
}

/// 密码预算属于用户窗口，传输重试只能丢掉当前尝试，不能重新开窗。
pub(super) struct Protocol {
    window: PairingWindow,
    state: SessionState,
    client: Option<String>,
    name: String,
    revision: u64,
}
impl Default for Protocol {
    fn default() -> Self {
        Self {
            window: PairingWindow::default(),
            state: SessionState::Disconnected,
            client: None,
            name: String::new(),
            revision: 0,
        }
    }
}
impl Protocol {
    pub(super) fn start_pairing(
        &mut self,
        client: &str,
        name: &str,
        code: String,
        revision: u64,
        now: Instant,
    ) -> Result<(), String> {
        self.cancel();
        self.window.open(code, now).map_err(|_| AUTH_ERROR)?;
        self.client = Some(client.into());
        // 本轮名称参与 transcript；用户修改展示名只影响下一次配对。
        self.name = name.into();
        self.revision = revision;
        Ok(())
    }
    pub(super) fn pairing_revision(&self) -> Option<u64> {
        self.client.as_ref().map(|_| self.revision)
    }
    pub(super) fn hello(
        &mut self,
        hello: PairingHello,
        device: &str,
        now: Instant,
    ) -> Result<PairingReply, String> {
        if self.client.as_deref() != Some(hello.client_id.as_str())
            || !matches!(self.state, SessionState::Disconnected)
        {
            return Err(AUTH_ERROR.into());
        }
        let (attempt, receiver, reply) = self
            .window
            .begin(
                hello,
                device,
                &self.name,
                uuid::Uuid::new_v4().to_string(),
                now,
            )
            .map_err(|_| AUTH_ERROR)?;
        self.state = SessionState::Pairing(attempt, receiver);
        Ok(reply)
    }
    pub(super) fn confirmation(
        &mut self,
        confirmation: PairingConfirmation,
        now: Instant,
    ) -> Result<Commit, String> {
        let SessionState::Pairing(attempt, receiver) =
            std::mem::replace(&mut self.state, SessionState::Disconnected)
        else {
            return Err(AUTH_ERROR.into());
        };
        if self.window.check(&attempt, now).is_err() {
            self.window.failed(attempt);
            return Err(AUTH_ERROR.into());
        }
        match receiver.verify_confirmation(confirmation) {
            Ok(pending) => Ok(Commit {
                attempt,
                pending,
                revision: self.revision,
            }),
            Err(_) => {
                self.window.failed(attempt);
                Err(AUTH_ERROR.into())
            }
        }
    }
    pub(super) fn resume(
        &mut self,
        client: &str,
        device: &str,
        secret: &[u8; 32],
    ) -> Result<SessionHello, String> {
        self.cancel();
        let (waiting, hello) =
            DeviceSession::start(client, device, secret, uuid::Uuid::new_v4().to_string())
                .map_err(|_| AUTH_ERROR)?;
        self.state = SessionState::Resuming(waiting);
        Ok(hello)
    }
    pub(super) fn resume_reply(
        &mut self,
        reply: SessionReply,
    ) -> Result<SessionConfirmation, String> {
        let SessionState::Resuming(waiting) =
            std::mem::replace(&mut self.state, SessionState::Disconnected)
        else {
            return Err(AUTH_ERROR.into());
        };
        let (channel, confirmation) = waiting.accept(reply).map_err(|_| AUTH_ERROR)?;
        self.state = SessionState::Connected(channel);
        Ok(confirmation)
    }
    pub(super) fn seal(&mut self, bytes: &[u8]) -> Result<EncryptedPacket, String> {
        let result = match &mut self.state {
            SessionState::Connected(channel) => {
                channel.seal(bytes).map_err(|_| AUTH_ERROR.to_owned())
            }
            _ => Err(AUTH_ERROR.into()),
        };
        if result.is_err() {
            self.disconnect();
        }
        result
    }
    pub(super) fn open(&mut self, packet: &EncryptedPacket) -> Result<Vec<u8>, String> {
        let result = match &mut self.state {
            SessionState::Connected(channel) => {
                channel.open(packet).map_err(|_| AUTH_ERROR.to_owned())
            }
            _ => Err(AUTH_ERROR.into()),
        };
        if result.is_err() {
            self.disconnect();
        }
        result
    }
    pub(super) fn disconnect(&mut self) {
        if let SessionState::Pairing(attempt, _) =
            std::mem::replace(&mut self.state, SessionState::Disconnected)
        {
            self.window.failed(attempt);
        }
    }
    pub(super) fn cancel(&mut self) {
        self.disconnect();
        self.window.close();
        self.client = None;
    }
}

/// 不可复制的本次保存资格，不需要跨语言桥的随机 commitId。
pub(super) struct Commit {
    attempt: PairingAttempt,
    pending: PendingReceiverCommit,
    revision: u64,
}
impl Commit {
    pub(super) fn material(&self) -> &PairingMaterial {
        self.pending.material()
    }
    pub(super) fn check(
        &self,
        protocol: &mut Protocol,
        revision: u64,
        now: Instant,
    ) -> Result<(), String> {
        if revision != self.revision || protocol.pairing_revision() != Some(self.revision) {
            return Err("配对已取消，请重新选择桌面。".into());
        }
        protocol
            .window
            .check(&self.attempt, now)
            .map_err(|_| "配对窗口已过期，请重新选择桌面。".into())
    }
    pub(super) fn abort(self, protocol: &mut Protocol) {
        protocol.window.failed(self.attempt);
    }
    pub(super) fn finish(
        self,
        protocol: &mut Protocol,
        revision: u64,
        now: Instant,
    ) -> Result<PairingAck, String> {
        if let Err(error) = self.check(protocol, revision, now) {
            self.abort(protocol);
            return Err(error);
        }
        protocol
            .window
            .saved(self.attempt, self.pending, true, now)
            .map_err(|_| AUTH_ERROR.into())
    }
}

#[cfg(test)]
#[path = "pairing_test.rs"]
mod pairing_test;
