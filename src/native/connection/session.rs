//! 配对、恢复和加密会话仍由同一个 owner 线程推进。
//! 拆分文件不会创建第二个密码状态机；序号消耗和 ACK 顺序继续由共享 Rust 核心决定。
// 引入材料、错误、套接字、owner、解码与快照校验
use super::{
    // 密钥材料与错误转换
    Material,
    // 错误转换
    err,
    // 带帧套接字及阶段
    framed_socket::{FramedSocket, Phase},
    // 当前毫秒时间
    now_ms,
    // 所有者
    owner::Owner,
    // 解码密钥
    persistence::decode_secret,
    // 快照校验
    snapshot_admission::validate_snapshot,
};
// 引入 JSON
use serde_json::{Value, json};
// 引入原子序与时间
use std::{
    // 原子操作的内存序
    sync::atomic::Ordering,
    // 时长与时刻
    time::{Duration, Instant},
};

// 握手阶段的超时：15 秒
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

// 所有者的方法
impl Owner {
    // 取得当前套接字，没有则报“连接已关闭”
    fn socket_mut(&mut self) -> Result<&mut FramedSocket, String> {
        // 取出套接字，已关闭则报错
        self.socket.as_mut().ok_or_else(|| "连接已关闭".into())
    }

    // 连接选中的桌面：配对用邀请，已信任用保存的秘密恢复
    pub(super) fn connect(&mut self) -> Result<(), String> {
        // 不活跃时不连接
        if !self.active() {
            // 没有变化直接返回
            return Ok(());
        }
        // 当前选中的桌面
        let id = self.selected.clone().ok_or("尚未选择桌面")?;
        // 该桌面当前的端点；必须唯一
        let values: Vec<_> = self.endpoints.values().filter(|e| e.id == id).collect();
        // 不可达或身份重复
        if values.len() != 1 {
            // 冷启动短暂失败/无结果之后,浏览器可能仍在运行。
            // 恢复重试时必须同时重试发现,而不仅仅是 TCP。
            // 冷启动时发现可能尚未结束；恢复重试时也要重试发现，而不只是 TCP
            if values.is_empty() && self.discovery.retry_at().is_none() {
                // 重新发现桌面
                self.rediscover();
            }
            // 桌面不可达或身份重复
            return Err("桌面不可达或存在重复身份，正在重新发现…".into());
        }
        // 取第一个候选的地址
        let address = values[0].address;
        // 是否为首次配对
        let pairing = self.pairing_offer.is_some();
        // 开场消息：配对邀请，或用保存的秘密生成的恢复请求
        let intro = if let Some(offer) = &self.pairing_offer {
            // 复制配对邀请
            offer.clone()
        // 否则分支
        } else {
            // 查找已保存的配对记录
            let record = self
                // 配对记录
                .records
                // 按标识查找
                .get(&id)
                // 找不到则要求重新选择桌面
                .ok_or("配对记录不可用，请重新选择桌面")?;
            // 解码密钥
            let secret = decode_secret(&record.secret).ok_or("安全存储记录无效")?;
            // 生成恢复握手
            let hello = self.protocol.resume(&id, &self.id, &secret)?;
            // 封装为恢复消息
            json!({"kind":"resume","value":hello})
        };
        // 丢弃旧连接
        self.drop_socket();
        // 新连接使用新代数
        self.generation += 1;
        // 建立连接
        let mut socket =
            // 连接并带上连接代次与事件通道
            FramedSocket::connect(address, pairing, self.generation, self.events.clone());
        // 配对时以配对窗口为期限
        if let Some(deadline) = self.pairing_deadline {
            // 设置连接期限
            socket.deadline = deadline;
        }
        // 发送开场消息
        socket.send(&intro)?;
        // 保存套接字
        self.socket = Some(socket);
        // 重置本连接的状态
        self.capabilities.clear();
        // 尚未上报设备形态
        self.form_factor_reported = false;
        // 重置资源状态
        self.assets.reset();
        // 视图显示为未连接并给出提示
        self.view["connected"] = json!(false);
        // 标记非实时
        self.view["live"] = json!(false);
        // 按是否配对设置状态
        self.view["status"] = json!(if pairing {
            // 等待输入配对码
            "等待在桌面输入手机配对码"
        // 否则分支
        } else {
            // 正在认证
            "正在认证桌面…"
        });
        // 发布视图
        self.publish();
        // 完成
        Ok(())
    }

    // 按当前阶段处理收到的消息
    pub(super) fn received(&mut self, v: Value) -> Result<(), String> {
        // 按连接阶段处理
        match self.socket_mut()?.phase {
            // 配对第一步：桌面问候，回复我方问候
            Phase::Hello => {
                // 发出握手
                let reply = self.protocol.hello(
                    // 解析问候
                    serde_json::from_value(v).map_err(err)?,
                    // 本机标识
                    &self.id,
                    // 当前时刻
                    Instant::now(),
                    // 问候处理完成
                )?;
                // 配对窗口必须仍有效
                let deadline = self.pairing_deadline.ok_or("配对已过期")?;
                // 取出套接字
                let w = self.socket_mut()?;
                // 进入确认阶段
                w.phase = Phase::Confirmation;
                // 期限不超过配对窗口
                w.deadline = (Instant::now() + HANDSHAKE_TIMEOUT).min(deadline);
                // 发送握手
                w.send(&serde_json::to_value(reply).map_err(err)?)?;
            }
            // 配对第二步：桌面确认
            Phase::Confirmation => self.confirmation(v)?,
            // 恢复第一步：校验桌面回复并发送确认
            Phase::Resume => {
                // 处理确认
                let confirmation = self
                    // 协议
                    .protocol
                    // 处理恢复回复
                    .resume_reply(serde_json::from_value(v).map_err(err)?)?;
                // 取出套接字
                let w = self.socket_mut()?;
                // 发送确认
                w.send(&serde_json::to_value(confirmation).map_err(err)?)?;
                // 进入推送阶段
                w.phase = Phase::Streaming;
                // 首条加密消息：上报设备信息
                self.send_secure(json!({"deviceId":self.id,"name":self.name,"platform":self.platform,"appVersion":self.app_version}))?;
                // 取出套接字
                let w = self.socket_mut()?;
                // 等待首份快照的期限
                w.deadline = Instant::now() + HANDSHAKE_TIMEOUT;
                // 记录最近一次收到数据的时刻
                w.last_inbound = Instant::now();
                // 更新发现状态
                self.update_discovery();
            }
            // 推送阶段：解密后按类型处理
            Phase::Streaming => {
                // 解密收到的数据
                let plain = self
                    // 协议
                    .protocol
                    // 解密收到的数据
                    .open(&serde_json::from_value(v).map_err(err)?)?;
                // 解析为 JSON 消息
                let message: Value = serde_json::from_slice(&plain).map_err(err)?;
                // 按消息类型处理
                match message["type"].as_str() {
                    // 心跳：回应 pong 并续租小组件
                    Some("ping") => {
                        // 心跳编号
                        let nonce = message["nonce"].as_u64().ok_or("无效心跳编号")?;
                        // 回复心跳
                        self.send_secure(json!({"type":"pong","nonce":nonce}))?;
                        // 续租小组件
                        self.renew_widget_lease();
                        // 发布视图
                        self.publish();
                    }
                    // 快照
                    Some("snapshot") => self.snapshot(message)?,
                    // 图片分块
                    Some("assetChunk") => self.asset(message["chunk"].clone())?,
                    // 桌面表示图片不可用
                    Some("assetUnavailable") => {
                        // 图片哈希
                        let hash = message["hash"].as_str().ok_or("无效图片响应")?;
                        // 标记该图片不可用
                        self.asset_unavailable(hash)?;
                    }
                    // 未知消息类型
                    _ => return Err("未知安全消息".into()),
                }
            }
        }
        // 完成
        Ok(())
    }

    /// 桌面确认后先真实保存，再生成 ACK；保存失败或期间被取消都不发送 ACK。
    // 桌面确认后先真实保存，再生成 ACK；保存失败或期间被取消都不发送 ACK
    fn confirmation(&mut self, v: Value) -> Result<(), String> {
        // 取出提交句柄
        let commit = self
            // 协议
            .protocol
            // 处理配对确认
            .confirmation(serde_json::from_value(v).map_err(err)?, Instant::now())?;
        // 检查期间有没有过期或被取消
        if let Err(error) = commit.check(
            // 协议
            &mut self.protocol,
            // 当前修订号
            self.revision.load(Ordering::SeqCst),
            // 当前时刻
            Instant::now(),
            // 检查失败
        ) {
            // 中止提交
            commit.abort(&mut self.protocol);
            // 返回错误
            return Err(error);
        }
        // 取出密钥材料
        let material = commit.material();
        // 组装要保存的配对材料，秘密以 base64 保存
        let record = Material {
            // 客户端标识
            client_id: material.client_id().into(),
            // 设备标识
            device_id: material.device_id().into(),
            // 设备名
            name: material.device_name().into(),
            // 密钥编码为 Base64
            secret: base64::Engine::encode(
                // 使用标准字母表
                &base64::engine::general_purpose::STANDARD,
                // 密钥字节
                material.secret_bytes(),
            ),
            // 保存时间
            saved_at_ms: now_ms(),
        };
        // 复制现有记录
        let mut records = self.records.clone();
        // 最多保存 64 个桌面
        if records.len() >= 64 && !records.contains_key(&record.client_id) {
            // 中止提交
            commit.abort(&mut self.protocol);
            // 配对记录达到上限
            return Err("最多保存64个桌面，请先忘记旧配对。".into());
        }
        // 保存这条记录
        records.insert(record.client_id.clone(), record);
        // 写入凭据库，失败则放弃
        if let Err(error) = self.save_records(&records) {
            // 中止提交
            commit.abort(&mut self.protocol);
            // 保存失败
            return Err(error);
        }
        // 已实际提交的记录必须保留，即使保存期间过期或已排队取消，仍不得伪造 ACK。
        // 已提交的记录必须保留，即使保存期间过期或已排队取消，也不伪造 ACK
        self.records = records;
        // 发布视图
        self.publish();
        // 生成 ACK
        let ack = commit.finish(
            // 协议
            &mut self.protocol,
            // 当前修订号
            self.revision.load(Ordering::SeqCst),
            // 当前时刻
            Instant::now(),
            // 检查完成
        )?;
        // 取出套接字
        let w = self.socket_mut()?;
        // 发送 ACK 并标记写完即关闭
        w.send(&serde_json::to_value(ack).map_err(err)?)?;
        // 标记配对完成后关闭
        w.paired_close = true;
        // 完成
        Ok(())
    }

    // 加密并发送一条消息
    pub(super) fn send_secure(&mut self, v: Value) -> Result<(), String> {
        // 在闭包里发送
        let result = (|| {
            // 序列化并加密
            let packet = self.protocol.seal(&serde_json::to_vec(&v).map_err(err)?)?;
            // 通过套接字发送
            self.socket_mut()?
                // 发送加密包
                .send(&serde_json::to_value(packet).map_err(err)?)
            // 闭包结束
        })();
        // seal 已消耗序号；排队失败时必须关闭整条通道，不能带着序号缺口继续发送。
        // seal 已消耗序号；排队失败必须关闭整条通道，不能带着序号缺口继续发送
        if let Err(error) = &result {
            // 发送失败，记录失败
            self.failed(error.clone());
        }
        // 返回结果
        result
    }

    // 处理快照：先校验，再更新视图、缓存与资源传输
    fn snapshot(&mut self, message: Value) -> Result<(), String> {
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
    fn report_form_factor(&mut self) -> Result<(), String> {
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

    /// 关闭当前 TCP，保留选择与信任；同一配对窗口内允许按退避重试。
    // 关闭当前 TCP，保留选择与信任；按退避重试
    pub(super) fn failed(&mut self, message: String) {
        // 此前是否有连接
        let had_transport = self.socket.is_some();
        // 丢弃套接字
        self.drop_socket();
        // 断开协议
        self.protocol.disconnect();
        // 清除实时数据
        self.clear_live();
        // 状态：连接中断，正在重试
        self.view["status"] = json!("连接中断，正在重试…");
        // 记录错误
        self.view["error"] = json!(message);
        // 有选择且活跃时安排重试
        if self.selected.is_some() && self.active() && (had_transport || self.retry_at.is_none()) {
            // 重试次数加一
            self.retries = self.retries.saturating_add(1);
            // 退避时间：500ms 起每次翻倍至 8 秒，超过 8 次后固定 30 秒
            self.retry_at = Some(
                // 当前时刻
                Instant::now()
                    // 加上退避时间
                    + if self.retries <= 8 {
                        // 退避时间
                        Duration::from_millis(
                            // 500 毫秒起指数增长
                            500u64.saturating_mul(1 << (self.retries.saturating_sub(1)).min(4)),
                        )
                    // 否则分支
                    } else {
                        // 上限为 30 秒
                        Duration::from_secs(30)
                    },
            );
        }
        // 更新发现状态
        self.update_discovery();
        // 发布视图
        self.publish();
    }

    /// 撤销连接、配对窗口与重试。
    // 撤销连接、配对窗口与重试
    pub(super) fn cancel(&mut self) {
        // 丢弃套接字
        self.drop_socket();
        // 清除重试时刻
        self.retry_at = None;
        // 清除配对状态
        self.clear_pairing();
        // 取消协议
        self.protocol.cancel();
    }

    // 丢弃套接字及其资源与能力
    pub(super) fn drop_socket(&mut self) {
        // 取走套接字
        if self.socket.take().is_some() {
            // 更新发现状态
            self.update_discovery();
        }
        // 重置资源状态
        self.assets.reset();
        // 清空能力集合
        self.capabilities.clear();
    }

    // 清除配对邀请、截止时间与视图中的配对码
    pub(super) fn clear_pairing(&mut self) {
        // 清除配对邀请
        self.pairing_offer = None;
        // 清除配对期限
        self.pairing_deadline = None;
        // 清除配对码
        self.view["pairingCode"] = Value::Null;
        // 清除配对期限显示
        self.view["pairingDeadlineMs"] = Value::Null;
    }

    // 清空展示内容
    pub(super) fn clear_display(&mut self) {
        // 标记未连接
        self.view["connected"] = json!(false);
        // 标记非实时
        self.view["live"] = json!(false);
        // 清除快照
        self.view["snapshot"] = Value::Null;
        // 清除接收时间
        self.view["receivedAtMs"] = Value::Null;
        // 清除缓存来源
        self.view["cacheSourceId"] = Value::Null;
        // 清空图片
        self.view["images"] = json!({});
    }

    /// 断线撤下实时状态；不保留离线快照的宿主同时清空展示。
    // 断线撤下实时状态；不保留离线快照的宿主同时清空展示
    pub(super) fn clear_live(&mut self) {
        // 标记未连接
        self.view["connected"] = json!(false);
        // 标记非实时
        self.view["live"] = json!(false);
        // 暂停小组件租约
        self.pause_widget_lease();
        // 不保留快照时
        if !self.keep_snapshot {
            // 清除显示
            self.clear_display();
        }
    }
}
