//! 配对、恢复和加密会话仍由同一个 owner 线程推进。
//! 拆分文件不会创建第二个密码状态机；序号消耗和 ACK 顺序继续由共享 Rust 核心决定。
use super::{
    // 密钥材料与错误转换
    Material,
    err,
    framed_socket::{FramedSocket, Phase},
    now_ms,
    owner::Owner,
    persistence::decode_secret,
    snapshot_admission::validate_snapshot,
};
use serde_json::{Value, json};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

mod snapshot;

// 握手阶段的超时：15 秒
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

impl Owner {
    // 取得当前套接字，没有则报“连接已关闭”
    fn socket_mut(&mut self) -> Result<&mut FramedSocket, String> {
        // 取出套接字，已关闭则报错
        self.socket.as_mut().ok_or_else(|| "连接已关闭".into())
    }

    // 连接选中的桌面：配对用邀请，已信任用保存的秘密恢复
    pub(super) fn connect(&mut self) -> Result<(), String> {
        if !self.active() {
            return Ok(());
        }
        let id = self.selected.clone().ok_or("尚未选择桌面")?;
        // 该桌面当前的端点；必须唯一
        let values: Vec<_> = self.endpoints.values().filter(|e| e.id == id).collect();
        if values.len() != 1 {
            // 冷启动短暂失败/无结果之后,浏览器可能仍在运行。
            // 恢复重试时必须同时重试发现,而不仅仅是 TCP。
            // 冷启动时发现可能尚未结束；恢复重试时也要重试发现，而不只是 TCP
            if values.is_empty() && self.discovery.retry_at().is_none() {
                self.rediscover();
            }
            // 桌面不可达或身份重复
            return Err("桌面不可达或存在重复身份，正在重新发现…".into());
        }
        // 取第一个候选的地址
        let address = values[0].address;
        let pairing = self.pairing_offer.is_some();
        // 开场消息：配对邀请，或用保存的秘密生成的恢复请求
        let intro = if let Some(offer) = &self.pairing_offer {
            offer.clone()
        // 否则分支
        } else {
            // 查找已保存的配对记录
            let record = self
                .records
                .get(&id)
                // 找不到则要求重新选择桌面
                .ok_or("配对记录不可用，请重新选择桌面")?;
            let secret = decode_secret(&record.secret).ok_or("安全存储记录无效")?;
            let hello = self.protocol.resume(&id, &self.id, &secret)?;
            json!({"kind":"resume","value":hello})
        };
        self.drop_socket();
        self.generation += 1;
        let mut socket =
            // 连接并带上连接代次与事件通道
            FramedSocket::connect(address, pairing, self.generation, self.events.clone());
        // 配对时以配对窗口为期限
        if let Some(deadline) = self.pairing_deadline {
            socket.deadline = deadline;
        }
        socket.send(&intro)?;
        self.socket = Some(socket);
        self.capabilities.clear();
        self.form_factor_reported = false;
        self.assets.reset();
        // 视图显示为未连接并给出提示
        self.view["connected"] = json!(false);
        self.view["live"] = json!(false);
        // 按是否配对设置状态
        self.view["status"] = json!(if pairing {
            "等待在桌面输入手机配对码"
        // 否则分支
        } else {
            "正在认证桌面…"
        });
        self.publish();
        Ok(())
    }

    // 按当前阶段处理收到的消息
    pub(super) fn received(&mut self, v: Value) -> Result<(), String> {
        match self.socket_mut()?.phase {
            // 配对第一步：桌面问候，回复我方问候
            Phase::Hello => {
                let reply = self.protocol.hello(
                    serde_json::from_value(v).map_err(err)?,
                    &self.id,
                    Instant::now(),
                )?;
                // 配对窗口必须仍有效
                let deadline = self.pairing_deadline.ok_or("配对已过期")?;
                let w = self.socket_mut()?;
                w.phase = Phase::Confirmation;
                // 期限不超过配对窗口
                w.deadline = (Instant::now() + HANDSHAKE_TIMEOUT).min(deadline);
                w.send(&serde_json::to_value(reply).map_err(err)?)?;
            }
            // 配对第二步：桌面确认
            Phase::Confirmation => self.confirmation(v)?,
            // 恢复第一步：校验桌面回复并发送确认
            Phase::Resume => {
                let confirmation = self
                    .protocol
                    .resume_reply(serde_json::from_value(v).map_err(err)?)?;
                let w = self.socket_mut()?;
                w.send(&serde_json::to_value(confirmation).map_err(err)?)?;
                w.phase = Phase::Streaming;
                // 首条加密消息：上报设备信息
                self.send_secure(json!({"deviceId":self.id,"name":self.name,"platform":self.platform,"appVersion":self.app_version}))?;
                let w = self.socket_mut()?;
                // 等待首份快照的期限
                w.deadline = Instant::now() + HANDSHAKE_TIMEOUT;
                // 记录最近一次收到数据的时刻
                w.last_inbound = Instant::now();
                self.update_discovery();
            }
            // 推送阶段：解密后按类型处理
            Phase::Streaming => {
                let plain = self
                    .protocol
                    .open(&serde_json::from_value(v).map_err(err)?)?;
                // 解析为 JSON 消息
                let message: Value = serde_json::from_slice(&plain).map_err(err)?;
                match message["type"].as_str() {
                    // 心跳：回应 pong 并续租小组件
                    Some("ping") => {
                        let nonce = message["nonce"].as_u64().ok_or("无效心跳编号")?;
                        self.send_secure(json!({"type":"pong","nonce":nonce}))?;
                        // 先pong再探测；只有实际WC回执才会稍后形成companionStatus。
                        #[cfg(any(target_os = "ios", test))]
                        self.watch_ping(nonce);
                        self.renew_widget_lease();
                        self.publish();
                    }
                    Some("snapshot") => self.snapshot(message)?,
                    Some("assetChunk") => self.asset(message["chunk"].clone())?,
                    // 桌面表示图片不可用
                    Some("assetUnavailable") => {
                        let hash = message["hash"].as_str().ok_or("无效图片响应")?;
                        self.asset_unavailable(hash)?;
                    }
                    _ => return Err("未知安全消息".into()),
                }
            }
        }
        Ok(())
    }

    /// 桌面确认后先真实保存，再生成 ACK；保存失败或期间被取消都不发送 ACK。
    // 桌面确认后先真实保存，再生成 ACK；保存失败或期间被取消都不发送 ACK
    fn confirmation(&mut self, v: Value) -> Result<(), String> {
        let commit = self
            .protocol
            .confirmation(serde_json::from_value(v).map_err(err)?, Instant::now())?;
        // 检查期间有没有过期或被取消
        if let Err(error) = commit.check(
            &mut self.protocol,
            self.revision.load(Ordering::SeqCst),
            Instant::now(),
        ) {
            commit.abort(&mut self.protocol);
            return Err(error);
        }
        let material = commit.material();
        // 组装要保存的配对材料，秘密以 base64 保存
        let record = Material {
            client_id: material.client_id().into(),
            device_id: material.device_id().into(),
            name: material.device_name().into(),
            // 密钥编码为 Base64
            secret: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                material.secret_bytes(),
            ),
            saved_at_ms: now_ms(),
        };
        let mut records = self.records.clone();
        // 最多保存 64 个桌面
        if records.len() >= 64 && !records.contains_key(&record.client_id) {
            commit.abort(&mut self.protocol);
            return Err("最多保存64个桌面，请先忘记旧配对。".into());
        }
        records.insert(record.client_id.clone(), record);
        // 写入凭据库，失败则放弃
        if let Err(error) = self.save_records(&records) {
            commit.abort(&mut self.protocol);
            return Err(error);
        }
        // 已实际提交的记录必须保留，即使保存期间过期或已排队取消，仍不得伪造 ACK。
        // 已提交的记录必须保留，即使保存期间过期或已排队取消，也不伪造 ACK
        self.records = records;
        self.publish();
        let ack = commit.finish(
            &mut self.protocol,
            self.revision.load(Ordering::SeqCst),
            Instant::now(),
        )?;
        let w = self.socket_mut()?;
        // 发送 ACK 并标记写完即关闭
        w.send(&serde_json::to_value(ack).map_err(err)?)?;
        // 标记配对完成后关闭
        w.paired_close = true;
        Ok(())
    }

    // 加密并发送一条消息
    pub(super) fn send_secure(&mut self, v: Value) -> Result<(), String> {
        let result = (|| {
            let packet = self.protocol.seal(&serde_json::to_vec(&v).map_err(err)?)?;
            self.socket_mut()?
                .send(&serde_json::to_value(packet).map_err(err)?)
        })();
        // seal 已消耗序号；排队失败时必须关闭整条通道，不能带着序号缺口继续发送。
        // seal 已消耗序号；排队失败必须关闭整条通道，不能带着序号缺口继续发送
        if let Err(error) = &result {
            // 发送失败，记录失败
            self.failed(error.clone());
        }
        result
    }

    /// 关闭当前 TCP，保留选择与信任；同一配对窗口内允许按退避重试。
    // 关闭当前 TCP，保留选择与信任；按退避重试
    pub(super) fn failed(&mut self, message: String) {
        let had_transport = self.socket.is_some();
        self.drop_socket();
        self.protocol.disconnect();
        self.clear_live();
        // 状态：连接中断，正在重试
        self.view["status"] = json!("连接中断，正在重试…");
        self.view["error"] = json!(message);
        // 有选择且活跃时安排重试
        if self.selected.is_some() && self.active() && (had_transport || self.retry_at.is_none()) {
            self.retries = self.retries.saturating_add(1);
            // 退避时间：500ms 起每次翻倍至 8 秒，超过 8 次后固定 30 秒
            self.retry_at = Some(
                Instant::now()
                    + if self.retries <= 8 {
                        Duration::from_millis(
                            // 500 毫秒起指数增长
                            500u64.saturating_mul(1 << (self.retries.saturating_sub(1)).min(4)),
                        )
                    // 否则分支
                    } else {
                        Duration::from_secs(30)
                    },
            );
        }
        self.update_discovery();
        self.publish();
    }

    /// 撤销连接、配对窗口与重试。
    // 撤销连接、配对窗口与重试
    pub(super) fn cancel(&mut self) {
        self.drop_socket();
        self.retry_at = None;
        self.clear_pairing();
        self.protocol.cancel();
    }

    // 丢弃套接字及其资源与能力
    pub(super) fn drop_socket(&mut self) {
        #[cfg(any(target_os = "ios", test))]
        self.watch.reset();
        if self.socket.take().is_some() {
            self.update_discovery();
        }
        self.assets.reset();
        self.capabilities.clear();
    }

    // 清除配对邀请、截止时间与视图中的配对码
    pub(super) fn clear_pairing(&mut self) {
        self.pairing_offer = None;
        self.pairing_deadline = None;
        self.view["pairingCode"] = Value::Null;
        self.view["pairingDeadlineMs"] = Value::Null;
    }

    pub(super) fn clear_display(&mut self) {
        self.view["connected"] = json!(false);
        self.view["live"] = json!(false);
        self.view["snapshot"] = Value::Null;
        self.view["receivedAtMs"] = Value::Null;
        self.view["cacheSourceId"] = Value::Null;
        self.view["images"] = json!({});
    }

    /// 断线撤下实时状态；不保留离线快照的宿主同时清空展示。
    // 断线撤下实时状态；不保留离线快照的宿主同时清空展示
    pub(super) fn clear_live(&mut self) {
        self.view["connected"] = json!(false);
        self.view["live"] = json!(false);
        self.pause_widget_lease();
        if !self.keep_snapshot {
            self.clear_display();
        }
    }
}
