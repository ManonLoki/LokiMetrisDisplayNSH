//! 桌面直接持有共享库的线性握手对象；这里只决定本机保存与用户取消的先后。
use loki_metris_pairing::{
    // 配对确认、问候、材料与回复类型
    PairingAck,
    PairingConfirmation,
    PairingHello,
    PairingMaterial,
    PairingReply,
    PendingReceiverCommit,
    ReceiverSession,
    session::{
        // 设备会话、加密包、安全信道、会话确认与问候
        DeviceSession,
        EncryptedPacket,
        SecureChannel,
        SessionConfirmation,
        SessionHello,
        SessionReply,
    },
    window::{PairingAttempt, PairingWindow},
};
use std::time::Instant;

// 认证失败的统一文案，不泄露具体原因
pub(super) const AUTH_ERROR: &str = "配对或安全认证失败，请重试。";

enum SessionState {
    Disconnected,
    Pairing(PairingAttempt, ReceiverSession),
    // 用保存的秘密恢复中
    Resuming(DeviceSession),
    Connected(SecureChannel),
}

/// 密码预算属于用户窗口，传输重试只能丢掉当前尝试，不能重新开窗。
// 协议状态：密码预算属于用户窗口，传输重试只能丢掉当前尝试，不能重新开窗
pub(super) struct Protocol {
    // 配对窗口（含尝试次数预算）
    window: PairingWindow,
    state: SessionState,
    client: Option<String>,
    // 本轮配对使用的名称
    name: String,
    // 配对开始时的取消代数
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
    // 开始配对：先取消旧的，再用配对码开窗
    pub(super) fn start_pairing(
        &mut self,
        client: &str,
        name: &str,
        code: String,
        revision: u64,
        now: Instant,
    ) -> Result<(), String> {
        // 先取消已有流程
        self.cancel();
        // 打开配对窗口，失败统一报认证错误
        self.window.open(code, now).map_err(|_| AUTH_ERROR)?;
        self.client = Some(client.into());
        // 本轮名称参与 transcript；用户修改展示名只影响下一次配对。
        // 本轮名称参与 transcript；用户中途修改展示名只影响下一次配对
        self.name = name.into();
        self.revision = revision;
        Ok(())
    }
    // 进行中的配对所属的取消代数
    pub(super) fn pairing_revision(&self) -> Option<u64> {
        // 有客户端时返回配对修订号
        self.client.as_ref().map(|_| self.revision)
    }
    // 处理桌面的问候：必须是目标客户端且当前未连接
    pub(super) fn hello(
        &mut self,
        hello: PairingHello,
        device: &str,
        now: Instant,
    ) -> Result<PairingReply, String> {
        // 客户端不是当前配对对象
        if self.client.as_deref() != Some(hello.client_id.as_str())
            // 或者不处于断开状态
            || !matches!(self.state, SessionState::Disconnected)
        {
            return Err(AUTH_ERROR.into());
        }
        // 开始一次尝试，得到回复
        let (attempt, receiver, reply) = self
            .window
            // 开始一次尝试
            .begin(
                hello,
                device,
                &self.name,
                uuid::Uuid::new_v4().to_string(),
                now,
            )
            // 失败统一报认证错误
            .map_err(|_| AUTH_ERROR)?;
        self.state = SessionState::Pairing(attempt, receiver);
        Ok(reply)
    }
    // 处理桌面的确认：校验窗口与确认码，返回待提交的保存资格
    pub(super) fn confirmation(
        &mut self,
        confirmation: PairingConfirmation,
        now: Instant,
    ) -> Result<Commit, String> {
        // 必须处于配对中
        let SessionState::Pairing(attempt, receiver) =
            // 取出旧状态并置为断开
            std::mem::replace(&mut self.state, SessionState::Disconnected)
        else {
            return Err(AUTH_ERROR.into());
        };
        // 窗口已过期或次数用尽
        if self.window.check(&attempt, now).is_err() {
            self.window.failed(attempt);
            return Err(AUTH_ERROR.into());
        }
        // 校验确认；失败计入尝试次数
        match receiver.verify_confirmation(confirmation) {
            // 成功则构造提交句柄
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
    // 用保存的秘密发起恢复，返回开场消息
    pub(super) fn resume(
        &mut self,
        client: &str,
        device: &str,
        secret: &[u8; 32],
    ) -> Result<SessionHello, String> {
        // 先取消已有流程
        self.cancel();
        let (waiting, hello) =
            DeviceSession::start(client, device, secret, uuid::Uuid::new_v4().to_string())
                // 失败统一报认证错误
                .map_err(|_| AUTH_ERROR)?;
        self.state = SessionState::Resuming(waiting);
        Ok(hello)
    }
    // 处理桌面对恢复的回复，成功则建立加密通道
    pub(super) fn resume_reply(
        &mut self,
        reply: SessionReply,
    ) -> Result<SessionConfirmation, String> {
        // 必须处于恢复状态
        let SessionState::Resuming(waiting) =
            // 取出旧状态并置为断开
            std::mem::replace(&mut self.state, SessionState::Disconnected)
        else {
            return Err(AUTH_ERROR.into());
        };
        // 接受回复，失败统一报认证错误
        let (channel, confirmation) = waiting.accept(reply).map_err(|_| AUTH_ERROR)?;
        self.state = SessionState::Connected(channel);
        Ok(confirmation)
    }
    // 加密一条消息；失败即断开
    pub(super) fn seal(&mut self, bytes: &[u8]) -> Result<EncryptedPacket, String> {
        // 仅已连接时加密
        let result = match &mut self.state {
            SessionState::Connected(channel) => {
                // 加密，失败统一报认证错误
                channel.seal(bytes).map_err(|_| AUTH_ERROR.to_owned())
            }
            _ => Err(AUTH_ERROR.into()),
        };
        if result.is_err() {
            self.disconnect();
        }
        result
    }
    // 解密一条消息；失败即断开
    pub(super) fn open(&mut self, packet: &EncryptedPacket) -> Result<Vec<u8>, String> {
        // 仅已连接时解密
        let result = match &mut self.state {
            SessionState::Connected(channel) => {
                // 解密，失败统一报认证错误
                channel.open(packet).map_err(|_| AUTH_ERROR.to_owned())
            }
            _ => Err(AUTH_ERROR.into()),
        };
        if result.is_err() {
            self.disconnect();
        }
        result
    }
    // 断开会话；配对中的尝试记为失败
    pub(super) fn disconnect(&mut self) {
        if let SessionState::Pairing(attempt, _) =
            // 取出旧状态并置为断开
            std::mem::replace(&mut self.state, SessionState::Disconnected)
        {
            self.window.failed(attempt);
        }
    }
    // 取消：断开、关窗并忘记配对对象
    pub(super) fn cancel(&mut self) {
        self.disconnect();
        self.window.close();
        self.client = None;
    }
}

/// 不可复制的本次保存资格，不需要跨语言桥的随机 commitId。
// 不可复制的本次保存资格，不需要跨语言桥的随机 commitId
pub(super) struct Commit {
    attempt: PairingAttempt,
    pending: PendingReceiverCommit,
    revision: u64,
}
impl Commit {
    pub(super) fn material(&self) -> &PairingMaterial {
        self.pending.material()
    }
    // 保存前检查：取消代数仍一致且窗口未过期
    pub(super) fn check(
        &self,
        protocol: &mut Protocol,
        revision: u64,
        now: Instant,
    ) -> Result<(), String> {
        // 修订号变了或配对已失效
        if revision != self.revision || protocol.pairing_revision() != Some(self.revision) {
            return Err("配对已取消，请重新选择桌面。".into());
        }
        protocol
            .window
            .check(&self.attempt, now)
            // 过期则提示重新选择桌面
            .map_err(|_| "配对窗口已过期，请重新选择桌面。".into())
    }
    // 放弃保存，记为失败
    pub(super) fn abort(self, protocol: &mut Protocol) {
        protocol.window.failed(self.attempt);
    }
    // 保存成功后生成 ACK；再检查一遍，过期或取消则不发 ACK
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
        // 标记已保存并生成 ACK
        protocol
            .window
            .saved(self.attempt, self.pending, true, now)
            // 失败统一报认证错误
            .map_err(|_| AUTH_ERROR.into())
    }
}

#[cfg(test)]
#[path = "pairing_test.rs"]
mod pairing_test;
