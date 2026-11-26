//! 在同一 owner 上接收配对、恢复与加密帧。
use super::Owner;
use crate::native::connection::{
    Material, err,
    framed_socket::{Phase, Progress},
    now_ms,
    snapshot_admission::validate_snapshot,
};
use serde_json::{Value, json};
use std::{sync::atomic::Ordering, time::Instant};

impl Owner {
    // 按当前阶段处理收到的消息
    pub(super) fn received(&mut self, v: Value) -> Result<(), String> {
        match self.socket_mut()?.phase() {
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
                w.progress(Progress::HelloReplied {
                    pairing_deadline: deadline,
                });
                w.send(&serde_json::to_value(reply).map_err(err)?)?;
            }
            // 配对第二步：桌面确认
            Phase::Confirmation | Phase::Acknowledging => self.confirmation(v)?,
            // 恢复第一步：校验桌面回复并发送确认
            Phase::Resume => {
                let confirmation = self
                    .protocol
                    .resume_reply(serde_json::from_value(v).map_err(err)?)?;
                let w = self.socket_mut()?;
                w.send(&serde_json::to_value(confirmation).map_err(err)?)?;
                w.progress(Progress::ResumeConfirmed);
                // 首条加密消息：上报设备信息
                self.send_secure(json!({"deviceId":self.id,"name":self.name,"platform":self.platform,"appVersion":self.app_version}))?;
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
            Phase::Pausing => {}
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
        w.progress(Progress::PairingAckQueued);
        Ok(())
    }

    // 处理快照：先校验，再更新视图、缓存与资源传输
    pub(super) fn snapshot(&mut self, message: Value) -> Result<(), String> {
        let snapshot = &message["snapshot"];
        // 校验失败则整体拒绝
        let descriptors = validate_snapshot(snapshot)?;
        // 记录桌面声明的能力
        self.capabilities = message["capabilities"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        self.report_form_factor()?;
        self.assets.replace(descriptors);
        // 清理已不在清单里的图片
        if let Some(images) = self.view["images"].as_object_mut() {
            // 只保留仍被引用的图片
            images.retain(|hash, _| self.assets.contains(hash));
        }
        let received = now_ms();
        self.share_widget_snapshot(&message, received);
        // 更新视图：已连接、实时、无错误
        self.view["snapshot"] = snapshot.clone();
        self.view["receivedAtMs"] = json!(received);
        self.view["cacheSourceId"] = json!(self.selected);
        self.view["live"] = json!(true);
        self.view["connected"] = json!(true);
        // 状态：已连接，正在同步
        self.view["status"] = json!("已连接，正在同步");
        self.view["error"] = Value::Null;
        // 已有快照后取消握手期限（空闲由 IDLE_TIMEOUT 管）
        self.socket_mut()?.progress(Progress::SnapshotAccepted);
        self.retries = 0;
        if self.keep_snapshot {
            self.write_snapshot();
        }
        // 开始下载缺失的图片
        self.next_asset()?;
        self.publish();
        Ok(())
    }

    /// 每个认证连接只在桌面声明支持后上报一次设备形态。
    // 每个认证连接只在桌面声明支持后上报一次设备形态
    pub(super) fn report_form_factor(&mut self) -> Result<(), String> {
        // 已上报过或对方不支持
        if self.form_factor_reported || !self.capabilities.contains("deviceFormFactorV1") {
            return Ok(());
        }
        let Some(form) = self.form_factor.clone() else {
            return Ok(());
        };
        self.form_factor_reported = true;
        // 发送加密的设备形态消息
        self.send_secure(json!({"type":"deviceFormFactor","formFactor":form}))
    }
}
