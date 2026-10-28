//! 桌面直接持有共享库的线性握手对象；这里只决定本机保存与用户取消的先后。
// 引入配对共享库的握手、会话与窗口类型
use loki_metris_pairing::{
    // 配对确认、问候、材料与回复类型
    PairingAck,
    // 配对确认
    PairingConfirmation,
    // 配对问候
    PairingHello,
    // 配对材料
    PairingMaterial,
    // 配对回复
    PairingReply,
    // 接收端待提交
    PendingReceiverCommit,
    // 接收端会话
    ReceiverSession,
    // 会话相关类型
    session::{
        // 设备会话、加密包、安全信道、会话确认与问候
        DeviceSession,
        // 加密包
        EncryptedPacket,
        // 安全信道
        SecureChannel,
        // 会话确认
        SessionConfirmation,
        // 会话问候
        SessionHello,
        // 会话回复
        SessionReply,
    },
    // 配对尝试与窗口
    window::{PairingAttempt, PairingWindow},
};
// 引入时间
use std::time::Instant;

// 认证失败的统一文案，不泄露具体原因
pub(super) const AUTH_ERROR: &str = "配对或安全认证失败，请重试。";

// 会话状态机
enum SessionState {
    // 未连接
    Disconnected,
    // 首次配对中
    Pairing(PairingAttempt, ReceiverSession),
    // 用保存的秘密恢复中
    Resuming(DeviceSession),
    // 已建立加密通道
    Connected(SecureChannel),
}

/// 密码预算属于用户窗口，传输重试只能丢掉当前尝试，不能重新开窗。
// 协议状态：密码预算属于用户窗口，传输重试只能丢掉当前尝试，不能重新开窗
pub(super) struct Protocol {
    // 配对窗口（含尝试次数预算）
    window: PairingWindow,
    // 会话状态
    state: SessionState,
    // 配对对象
    client: Option<String>,
    // 本轮配对使用的名称
    name: String,
    // 配对开始时的取消代数
    revision: u64,
}
// 协议的默认实现
impl Default for Protocol {
    // 默认值
    fn default() -> Self {
        // 构造协议
        Self {
            // 默认配对窗口
            window: PairingWindow::default(),
            // 初始为断开
            state: SessionState::Disconnected,
            // 无客户端
            client: None,
            // 设备名为空
            name: String::new(),
            // 修订号为 0
            revision: 0,
        }
    }
}
// 协议的方法
impl Protocol {
    // 开始配对：先取消旧的，再用配对码开窗
    pub(super) fn start_pairing(
        // 自身
        &mut self,
        // 客户端
        client: &str,
        // 名称
        name: &str,
        // 配对码
        code: String,
        // 修订号
        revision: u64,
        // 当前时刻
        now: Instant,
        // 返回结果
    ) -> Result<(), String> {
        // 先取消已有流程
        self.cancel();
        // 打开配对窗口，失败统一报认证错误
        self.window.open(code, now).map_err(|_| AUTH_ERROR)?;
        // 记下客户端
        self.client = Some(client.into());
        // 本轮名称参与 transcript；用户修改展示名只影响下一次配对。
        // 本轮名称参与 transcript；用户中途修改展示名只影响下一次配对
        self.name = name.into();
        // 记录修订号
        self.revision = revision;
        // 完成
        Ok(())
    }
    // 进行中的配对所属的取消代数
    pub(super) fn pairing_revision(&self) -> Option<u64> {
        // 有客户端时返回配对修订号
        self.client.as_ref().map(|_| self.revision)
    }
    // 处理桌面的问候：必须是目标客户端且当前未连接
    pub(super) fn hello(
        // 自身
        &mut self,
        // 问候
        hello: PairingHello,
        // 设备
        device: &str,
        // 当前时刻
        now: Instant,
        // 返回回复
    ) -> Result<PairingReply, String> {
        // 客户端不是当前配对对象
        if self.client.as_deref() != Some(hello.client_id.as_str())
            // 或者不处于断开状态
            || !matches!(self.state, SessionState::Disconnected)
        {
            // 统一报认证错误
            return Err(AUTH_ERROR.into());
        }
        // 开始一次尝试，得到回复
        let (attempt, receiver, reply) = self
            // 配对窗口
            .window
            // 开始一次尝试
            .begin(
                // 问候
                hello,
                // 设备
                device,
                // 名称
                &self.name,
                // 随机会话标识
                uuid::Uuid::new_v4().to_string(),
                // 当前时刻
                now,
            )
            // 失败统一报认证错误
            .map_err(|_| AUTH_ERROR)?;
        // 进入配对中状态
        self.state = SessionState::Pairing(attempt, receiver);
        // 返回回复
        Ok(reply)
    }
    // 处理桌面的确认：校验窗口与确认码，返回待提交的保存资格
    pub(super) fn confirmation(
        // 自身
        &mut self,
        // 确认
        confirmation: PairingConfirmation,
        // 当前时刻
        now: Instant,
        // 返回提交句柄
    ) -> Result<Commit, String> {
        // 必须处于配对中
        let SessionState::Pairing(attempt, receiver) =
            // 取出旧状态并置为断开
            std::mem::replace(&mut self.state, SessionState::Disconnected)
        // 旧状态不是配对中
        else {
            // 统一报认证错误
            return Err(AUTH_ERROR.into());
        };
        // 窗口已过期或次数用尽
        if self.window.check(&attempt, now).is_err() {
            // 记录配对失败
            self.window.failed(attempt);
            // 统一报认证错误
            return Err(AUTH_ERROR.into());
        }
        // 校验确认；失败计入尝试次数
        match receiver.verify_confirmation(confirmation) {
            // 成功则构造提交句柄
            Ok(pending) => Ok(Commit {
                // 本次尝试
                attempt,
                // 待提交
                pending,
                // 修订号
                revision: self.revision,
            }),
            // 其余错误
            Err(_) => {
                // 记录配对失败
                self.window.failed(attempt);
                // 统一报认证错误
                Err(AUTH_ERROR.into())
            }
        }
    }
    // 用保存的秘密发起恢复，返回开场消息
    pub(super) fn resume(
        // 自身
        &mut self,
        // 客户端
        client: &str,
        // 设备
        device: &str,
        // 密钥
        secret: &[u8; 32],
        // 返回会话问候
    ) -> Result<SessionHello, String> {
        // 先取消已有流程
        self.cancel();
        // 开始恢复会话
        let (waiting, hello) =
            // 开始设备会话
            DeviceSession::start(client, device, secret, uuid::Uuid::new_v4().to_string())
                // 失败统一报认证错误
                .map_err(|_| AUTH_ERROR)?;
        // 进入恢复中状态
        self.state = SessionState::Resuming(waiting);
        // 返回握手消息
        Ok(hello)
    }
    // 处理桌面对恢复的回复，成功则建立加密通道
    pub(super) fn resume_reply(
        // 自身
        &mut self,
        // 回复
        reply: SessionReply,
        // 返回会话确认
    ) -> Result<SessionConfirmation, String> {
        // 必须处于恢复状态
        let SessionState::Resuming(waiting) =
            // 取出旧状态并置为断开
            std::mem::replace(&mut self.state, SessionState::Disconnected)
        // 旧状态不是恢复中
        else {
            // 统一报认证错误
            return Err(AUTH_ERROR.into());
        };
        // 接受回复，失败统一报认证错误
        let (channel, confirmation) = waiting.accept(reply).map_err(|_| AUTH_ERROR)?;
        // 进入已连接状态
        self.state = SessionState::Connected(channel);
        // 返回确认消息
        Ok(confirmation)
    }
    // 加密一条消息；失败即断开
    pub(super) fn seal(&mut self, bytes: &[u8]) -> Result<EncryptedPacket, String> {
        // 仅已连接时加密
        let result = match &mut self.state {
            // 已连接
            SessionState::Connected(channel) => {
                // 加密，失败统一报认证错误
                channel.seal(bytes).map_err(|_| AUTH_ERROR.to_owned())
            }
            // 其他状态
            _ => Err(AUTH_ERROR.into()),
        };
        // 失败则断开连接
        if result.is_err() {
            // 断开连接
            self.disconnect();
        }
        // 返回结果
        result
    }
    // 解密一条消息；失败即断开
    pub(super) fn open(&mut self, packet: &EncryptedPacket) -> Result<Vec<u8>, String> {
        // 仅已连接时解密
        let result = match &mut self.state {
            // 已连接
            SessionState::Connected(channel) => {
                // 解密，失败统一报认证错误
                channel.open(packet).map_err(|_| AUTH_ERROR.to_owned())
            }
            // 其他状态
            _ => Err(AUTH_ERROR.into()),
        };
        // 失败则断开连接
        if result.is_err() {
            // 断开连接
            self.disconnect();
        }
        // 返回结果
        result
    }
    // 断开会话；配对中的尝试记为失败
    pub(super) fn disconnect(&mut self) {
        // 处于配对中
        if let SessionState::Pairing(attempt, _) =
            // 取出旧状态并置为断开
            std::mem::replace(&mut self.state, SessionState::Disconnected)
        {
            // 记录配对失败
            self.window.failed(attempt);
        }
    }
    // 取消：断开、关窗并忘记配对对象
    pub(super) fn cancel(&mut self) {
        // 断开连接
        self.disconnect();
        // 关闭配对窗口
        self.window.close();
        // 清除客户端
        self.client = None;
    }
}

/// 不可复制的本次保存资格，不需要跨语言桥的随机 commitId。
// 不可复制的本次保存资格，不需要跨语言桥的随机 commitId
pub(super) struct Commit {
    // 本次尝试
    attempt: PairingAttempt,
    // 待提交
    pending: PendingReceiverCommit,
    // 修订号
    revision: u64,
}
// 提交句柄的方法
impl Commit {
    // 待保存的配对材料
    pub(super) fn material(&self) -> &PairingMaterial {
        // 取出配对材料
        self.pending.material()
    }
    // 保存前检查：取消代数仍一致且窗口未过期
    pub(super) fn check(
        // 自身
        &self,
        // 协议
        protocol: &mut Protocol,
        // 修订号
        revision: u64,
        // 当前时刻
        now: Instant,
        // 返回结果
    ) -> Result<(), String> {
        // 修订号变了或配对已失效
        if revision != self.revision || protocol.pairing_revision() != Some(self.revision) {
            // 配对已取消
            return Err("配对已取消，请重新选择桌面。".into());
        }
        // 协议
        protocol
            // 配对窗口
            .window
            // 检查本次尝试
            .check(&self.attempt, now)
            // 过期则提示重新选择桌面
            .map_err(|_| "配对窗口已过期，请重新选择桌面。".into())
    }
    // 放弃保存，记为失败
    pub(super) fn abort(self, protocol: &mut Protocol) {
        // 记录配对失败
        protocol.window.failed(self.attempt);
    }
    // 保存成功后生成 ACK；再检查一遍，过期或取消则不发 ACK
    pub(super) fn finish(
        // 消耗自身
        self,
        // 协议
        protocol: &mut Protocol,
        // 修订号
        revision: u64,
        // 当前时刻
        now: Instant,
        // 返回配对确认
    ) -> Result<PairingAck, String> {
        // 检查失败
        if let Err(error) = self.check(protocol, revision, now) {
            // 中止提交
            self.abort(protocol);
            // 返回错误
            return Err(error);
        }
        // 标记已保存并生成 ACK
        protocol
            // 配对窗口
            .window
            // 标记已保存
            .saved(self.attempt, self.pending, true, now)
            // 失败统一报认证错误
            .map_err(|_| AUTH_ERROR.into())
    }
}

// 测试模块
#[cfg(test)]
#[path = "pairing_test.rs"]
mod pairing_test;
