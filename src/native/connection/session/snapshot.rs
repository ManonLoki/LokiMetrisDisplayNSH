use super::*;

impl Owner {
    // 处理快照：先校验，再更新视图、缓存与资源传输
    pub(super) fn snapshot(&mut self, message: Value) -> Result<(), String> {
        // 快照内容
        let snapshot = &message["snapshot"];
        // 校验失败则整体拒绝
        let descriptors = validate_snapshot(snapshot)?;
        // 记录桌面声明的能力
        self.capabilities = message["capabilities"]
            // 转为数组
            .as_array()
            // 转为迭代器
            .into_iter()
            // 展平
            .flatten()
            // 取字符串
            .filter_map(Value::as_str)
            // 转为自有字符串
            .map(str::to_owned)
            // 收集为集合
            .collect();
        // 上报设备形态
        self.report_form_factor()?;
        // 更新资源清单
        self.assets.replace(descriptors);
        // 清理已不在清单里的图片
        if let Some(images) = self.view["images"].as_object_mut() {
            // 只保留仍被引用的图片
            images.retain(|hash, _| self.assets.contains(hash));
        }
        // 接收时刻
        let received = now_ms();
        // 共享给小组件
        self.share_widget_snapshot(&message, received);
        // 更新视图：已连接、实时、无错误
        self.view["snapshot"] = snapshot.clone();
        // 记录接收时间
        self.view["receivedAtMs"] = json!(received);
        // 缓存来源
        self.view["cacheSourceId"] = json!(self.selected);
        // 标记实时
        self.view["live"] = json!(true);
        // 标记已连接
        self.view["connected"] = json!(true);
        // 状态：已连接，正在同步
        self.view["status"] = json!("已连接，正在同步");
        // 清除错误
        self.view["error"] = Value::Null;
        // 已有快照后取消握手期限（空闲由 IDLE_TIMEOUT 管）
        self.socket_mut()?.deadline = Instant::now() + Duration::from_secs(365 * 86400);
        // 重试计数归零
        self.retries = 0;
        // 需要时落盘缓存
        if self.keep_snapshot {
            // 写入快照缓存
            self.write_snapshot();
        }
        // 开始下载缺失的图片
        self.next_asset()?;
        // 发布视图
        self.publish();
        // 完成
        Ok(())
    }

    /// 每个认证连接只在桌面声明支持后上报一次设备形态。
    // 每个认证连接只在桌面声明支持后上报一次设备形态
    pub(super) fn report_form_factor(&mut self) -> Result<(), String> {
        // 已上报过或对方不支持
        if self.form_factor_reported || !self.capabilities.contains("deviceFormFactorV1") {
            // 直接返回
            return Ok(());
        }
        // 没有设备形态
        let Some(form) = self.form_factor.clone() else {
            // 直接返回
            return Ok(());
        };
        // 标记已上报
        self.form_factor_reported = true;
        // 发送加密的设备形态消息
        self.send_secure(json!({"type":"deviceFormFactor","formFactor":form}))
    }
}
