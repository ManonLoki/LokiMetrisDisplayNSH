use super::*;

impl Owner {
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
        self.socket_mut()?.deadline = Instant::now() + Duration::from_secs(365 * 86400);
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
