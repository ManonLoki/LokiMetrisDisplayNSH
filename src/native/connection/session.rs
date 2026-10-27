//! 配对、恢复和加密会话仍由同一个 owner 线程推进。
//! 拆分文件不会创建第二个密码状态机；序号消耗和 ACK 顺序继续由共享 Rust 核心决定。
use super::{
    Material, err,
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

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

impl Owner {
    fn socket_mut(&mut self) -> Result<&mut FramedSocket, String> {
        self.socket.as_mut().ok_or_else(|| "连接已关闭".into())
    }

    pub(super) fn connect(&mut self) -> Result<(), String> {
        if !self.active() {
            return Ok(());
        }
        let id = self.selected.clone().ok_or("尚未选择桌面")?;
        let values: Vec<_> = self.endpoints.values().filter(|e| e.id == id).collect();
        if values.len() != 1 {
            // A browser may still be running after a transient/no-result cold
            // launch. Resume retries must also retry discovery, not just TCP.
            if values.is_empty() && self.discovery.retry_at().is_none() {
                self.rediscover();
            }
            return Err("桌面不可达或存在重复身份，正在重新发现…".into());
        }
        let address = values[0].address;
        let pairing = self.pairing_offer.is_some();
        let intro = if let Some(offer) = &self.pairing_offer {
            offer.clone()
        } else {
            let record = self
                .records
                .get(&id)
                .ok_or("配对记录不可用，请重新选择桌面")?;
            let secret = decode_secret(&record.secret).ok_or("安全存储记录无效")?;
            let hello = self.protocol.resume(&id, &self.id, &secret)?;
            json!({"kind":"resume","value":hello})
        };
        self.drop_socket();
        self.generation += 1;
        let mut socket =
            FramedSocket::connect(address, pairing, self.generation, self.events.clone());
        if let Some(deadline) = self.pairing_deadline {
            socket.deadline = deadline;
        }
        socket.send(&intro)?;
        self.socket = Some(socket);
        self.capabilities.clear();
        self.form_factor_reported = false;
        self.assets.reset();
        self.view["connected"] = json!(false);
        self.view["live"] = json!(false);
        self.view["status"] = json!(if pairing {
            "等待在桌面输入手机配对码"
        } else {
            "正在认证桌面…"
        });
        self.publish();
        Ok(())
    }

    pub(super) fn received(&mut self, v: Value) -> Result<(), String> {
        match self.socket_mut()?.phase {
            Phase::Hello => {
                let reply = self.protocol.hello(
                    serde_json::from_value(v).map_err(err)?,
                    &self.id,
                    Instant::now(),
                )?;
                let deadline = self.pairing_deadline.ok_or("配对已过期")?;
                let w = self.socket_mut()?;
                w.phase = Phase::Confirmation;
                w.deadline = (Instant::now() + HANDSHAKE_TIMEOUT).min(deadline);
                w.send(&serde_json::to_value(reply).map_err(err)?)?;
            }
            Phase::Confirmation => self.confirmation(v)?,
            Phase::Resume => {
                let confirmation = self
                    .protocol
                    .resume_reply(serde_json::from_value(v).map_err(err)?)?;
                let w = self.socket_mut()?;
                w.send(&serde_json::to_value(confirmation).map_err(err)?)?;
                w.phase = Phase::Streaming;
                self.send_secure(json!({"deviceId":self.id,"name":self.name,"platform":self.platform,"appVersion":self.app_version}))?;
                let w = self.socket_mut()?;
                w.deadline = Instant::now() + HANDSHAKE_TIMEOUT;
                w.last_inbound = Instant::now();
                self.update_discovery();
            }
            Phase::Streaming => {
                let plain = self
                    .protocol
                    .open(&serde_json::from_value(v).map_err(err)?)?;
                let message: Value = serde_json::from_slice(&plain).map_err(err)?;
                match message["type"].as_str() {
                    Some("ping") => {
                        let nonce = message["nonce"].as_u64().ok_or("无效心跳编号")?;
                        self.send_secure(json!({"type":"pong","nonce":nonce}))?;
                        self.publish();
                    }
                    Some("snapshot") => self.snapshot(message)?,
                    Some("assetChunk") => self.asset(message["chunk"].clone())?,
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
    fn confirmation(&mut self, v: Value) -> Result<(), String> {
        let commit = self
            .protocol
            .confirmation(serde_json::from_value(v).map_err(err)?, Instant::now())?;
        if let Err(error) = commit.check(
            &mut self.protocol,
            self.revision.load(Ordering::SeqCst),
            Instant::now(),
        ) {
            commit.abort(&mut self.protocol);
            return Err(error);
        }
        let material = commit.material();
        let record = Material {
            client_id: material.client_id().into(),
            device_id: material.device_id().into(),
            name: material.device_name().into(),
            secret: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                material.secret_bytes(),
            ),
            saved_at_ms: now_ms(),
        };
        let mut records = self.records.clone();
        if records.len() >= 64 && !records.contains_key(&record.client_id) {
            commit.abort(&mut self.protocol);
            return Err("最多保存64个桌面，请先忘记旧配对。".into());
        }
        records.insert(record.client_id.clone(), record);
        if let Err(error) = self.save_records(&records) {
            commit.abort(&mut self.protocol);
            return Err(error);
        }
        // 已实际提交的记录必须保留，即使保存期间过期或已排队取消，仍不得伪造 ACK。
        self.records = records;
        self.publish();
        let ack = commit.finish(
            &mut self.protocol,
            self.revision.load(Ordering::SeqCst),
            Instant::now(),
        )?;
        let w = self.socket_mut()?;
        w.send(&serde_json::to_value(ack).map_err(err)?)?;
        w.paired_close = true;
        Ok(())
    }

    pub(super) fn send_secure(&mut self, v: Value) -> Result<(), String> {
        let result = (|| {
            let packet = self.protocol.seal(&serde_json::to_vec(&v).map_err(err)?)?;
            self.socket_mut()?
                .send(&serde_json::to_value(packet).map_err(err)?)
        })();
        // seal 已消耗序号；排队失败时必须关闭整条通道，不能带着序号缺口继续发送。
        if let Err(error) = &result {
            self.failed(error.clone());
        }
        result
    }

    fn snapshot(&mut self, message: Value) -> Result<(), String> {
        let snapshot = &message["snapshot"];
        let descriptors = validate_snapshot(snapshot)?;
        self.capabilities = message["capabilities"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        self.report_form_factor()?;
        self.assets.replace(descriptors);
        if let Some(images) = self.view["images"].as_object_mut() {
            images.retain(|hash, _| self.assets.contains(hash));
        }
        let received = now_ms();
        self.view["snapshot"] = snapshot.clone();
        self.view["receivedAtMs"] = json!(received);
        self.view["cacheSourceId"] = json!(self.selected);
        self.view["live"] = json!(true);
        self.view["connected"] = json!(true);
        self.view["status"] = json!("已连接，正在同步");
        self.view["error"] = Value::Null;
        self.socket_mut()?.deadline = Instant::now() + Duration::from_secs(365 * 86400);
        self.retries = 0;
        if self.keep_snapshot {
            self.write_snapshot();
        }
        self.next_asset()?;
        self.publish();
        Ok(())
    }

    /// 每个认证连接只在桌面声明支持后上报一次设备形态。
    fn report_form_factor(&mut self) -> Result<(), String> {
        if self.form_factor_reported || !self.capabilities.contains("deviceFormFactorV1") {
            return Ok(());
        }
        let Some(form) = self.form_factor.clone() else {
            return Ok(());
        };
        self.form_factor_reported = true;
        self.send_secure(json!({"type":"deviceFormFactor","formFactor":form}))
    }

    /// 关闭当前 TCP，保留选择与信任；同一配对窗口内允许按退避重试。
    pub(super) fn failed(&mut self, message: String) {
        let had_transport = self.socket.is_some();
        self.drop_socket();
        self.protocol.disconnect();
        self.clear_live();
        self.view["status"] = json!("连接中断，正在重试…");
        self.view["error"] = json!(message);
        if self.selected.is_some() && self.active() && (had_transport || self.retry_at.is_none()) {
            self.retries = self.retries.saturating_add(1);
            self.retry_at = Some(
                Instant::now()
                    + if self.retries <= 8 {
                        Duration::from_millis(
                            500u64.saturating_mul(1 << (self.retries.saturating_sub(1)).min(4)),
                        )
                    } else {
                        Duration::from_secs(30)
                    },
            );
        }
        self.update_discovery();
        self.publish();
    }

    /// 撤销连接、配对窗口与重试。
    pub(super) fn cancel(&mut self) {
        self.drop_socket();
        self.retry_at = None;
        self.clear_pairing();
        self.protocol.cancel();
    }

    pub(super) fn drop_socket(&mut self) {
        if self.socket.take().is_some() {
            self.update_discovery();
        }
        self.assets.reset();
        self.capabilities.clear();
    }

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
    pub(super) fn clear_live(&mut self) {
        self.view["connected"] = json!(false);
        self.view["live"] = json!(false);
        if !self.keep_snapshot {
            self.clear_display();
        }
    }
}
