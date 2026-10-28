use super::*;

impl Actor {
    // 推进同步：激活会话、发送上下文、传输缺失的图片
    pub(super) fn flush(&mut self) {
        // 当前时刻
        let now = Instant::now();
        // 会话需要激活时
        if self
            // 激活期限
            .activation_deadline
            // 已到
            .is_some_and(|deadline| now >= deadline)
        {
            // 清除激活期限
            self.activation_deadline = None;
            // 登记激活失败
            self.activation_retry.failed(now);
        }
        // 未激活：仅在未激活（0）状态才发起激活；不活跃（1）需先完成停用，才能为另一块手表重新激活
        if self.session.activation_state() != 2 {
            // Inactive (1) 必须先完成停用,才能为另一只手表重新激活。
            if self.session.activation_state() == 0
                // 且没有激活期限
                && self.activation_deadline.is_none()
                // 且激活重试已就绪
                && self.activation_retry.ready(now)
            {
                // 记录激活已开始
                self.activation_retry.started();
                // 设 15 秒的激活期限
                self.activation_deadline = Some(now + Duration::from_secs(15));
                // 激活会话
                self.session.activate();
            }
            // 然后返回
            return;
        }
        // 没有配对手表或手表应用未安装：取消所有传输并清除上下文
        if !self.session.paired() || !self.session.installed() {
            // 取消所有进行中的传输
            self.cancel_outstanding(None);
            // 清空上下文
            self.context = None;
            // 结束本轮
            return;
        }
        // 清单不可读则等待下次
        let Ok(manifest) = read_manifest(&self.root) else {
            // 结束本轮
            return;
        };
        // 数据流或来源变化
        let switched = self.manifest.as_ref().is_some_and(|previous| {
            // 流或来源与之前不同
            previous.stream_id != manifest.stream_id || previous.source_id != manifest.source_id
        });
        // 切换来源或清空：取消全部传输并清空状态
        if switched || manifest.snapshot.is_none() {
            // 取消所有进行中的传输
            self.cancel_outstanding(None);
            // 清空已成功的传输
            self.successful.clear();
            // 清空尝试次数
            self.attempts.clear();
            // 清空本地失败
            self.local_failures.clear();
        // 否则只取消与新清单不兼容的传输
        } else {
            // 只取消不在新清单里的传输
            self.cancel_outstanding(Some(&manifest));
        }
        // 清单变化：重发上下文，并裁剪不再引用的资源状态
        if self.manifest.as_ref() != Some(&manifest) {
            // 清空上下文
            self.context = None;
            // 重置上下文重试
            self.context_retry = Retry::default();
            // 已成功的传输
            self.successful
                // 只保留清单里仍有的哈希
                .retain(|hash| manifest.assets.iter().any(|asset| &asset.hash == hash));
            // 尝试次数
            self.attempts
                // 只保留清单里仍有的传输
                .retain(|hash, _| manifest.assets.iter().any(|asset| &asset.hash == hash));
            // 本地失败
            self.local_failures
                // 只保留仍在清单里的资源
                .retain(|hash| manifest.assets.iter().any(|asset| &asset.hash == hash));
            // 记下新清单
            self.manifest = Some(manifest.clone());
        }
        // 发送应用上下文（不超过 48 KiB）；失败则退避
        if self.context.is_none() && self.context_retry.ready(now) {
            // 记录上下文发送开始
            self.context_retry.started();
            // 序列化清单
            if let Ok(bytes) = serde_json::to_vec(&manifest) {
                // 不超过 48 KiB 且发送成功
                if bytes.len() <= 48 * 1024 && self.session.context(&bytes).is_ok() {
                    // 记住已发送的上下文
                    self.context = Some(bytes);
                // 否则
                } else {
                    // 登记失败，稍后重试
                    self.context_retry.failed(now);
                }
            }
        }
        // 已在传输的摘要
        let mut occupied: BTreeSet<_> = self
            // 进行中的传输
            .in_flight
            // 取其值
            .values()
            // 取哈希
            .map(|transfer| transfer.hash.clone())
            // 收集为集合
            .collect();
        // 逐个资源安排传输，最多同时 MAX_TRANSFERS 个
        for asset in &manifest.assets {
            // 进行中的传输已达上限
            if self.in_flight.len() >= MAX_TRANSFERS {
                // 停止发起新传输
                break;
            }
            // 已成功、传输中或尚在退避的跳过
            if self.successful.contains(&asset.hash)
                // 或者该哈希已被占用
                || occupied.contains(&asset.hash)
                // 或者
                || !self
                    // 尝试记录
                    .attempts
                    // 按哈希取条目
                    .entry(asset.hash.clone())
                    // 默认策略
                    .or_default()
                    // 尚未到重试时刻
                    .ready(now)
            {
                // 跳过本项
                continue;
            }
            // 记录开始一次尝试
            self.attempts
                // 按哈希取条目
                .entry(asset.hash.clone())
                // 默认策略
                .or_default()
                // 记录开始尝试
                .started();
            // 暂存不可变副本
            match files::stage(&self.root, asset) {
                // 文件准备成功
                Ok(path) => {
                    // 清除本地失败记录
                    self.local_failures.remove(&asset.hash);
                    // 暂存可能耗时;所有者上的来源切换必须优先。
                    // 暂存可能耗时；期间 owner 切换了来源就丢弃，以新状态为准
                    if !read_manifest(&self.root).is_ok_and(|latest| {
                        // 最新清单仍属于同一个流
                        latest.stream_id == manifest.stream_id
                            // 来源一致
                            && latest.source_id == manifest.source_id
                            // 资源仍在清单里
                            && latest.assets.iter().any(|candidate| candidate == asset)
                        // 条件结束
                    }) {
                        // 否则删除这份暂存文件
                        files::remove(&self.root, &path);
                        // 唤醒执行者
                        wake();
                        // 放弃本次传输
                        return;
                    }
                    // 构造传输描述
                    let transfer = Transfer {
                        // 所属流
                        stream: manifest.stream_id.clone(),
                        // 修订号
                        revision: manifest.revision.parse().unwrap_or(0),
                        // 资源哈希
                        hash: asset.hash.clone(),
                        // 文件路径
                        path,
                    };
                    // 传输元数据：类型、流 ID、修订号与摘要
                    let keys: Vec<_> = ["kind", "streamId", "revision", "hash"]
                        // 遍历
                        .iter()
                        // 转为系统字符串
                        .map(|key| NSString::from_str(key))
                        // 收集为列表
                        .collect();
                    // 键与值的列表
                    let values: Vec<_> = [
                        // 种类：资源
                        "asset",
                        // 流标识
                        &manifest.stream_id,
                        // 修订号
                        &manifest.revision,
                        // 资源哈希
                        &asset.hash,
                    ]
                    // 遍历
                    .iter()
                    // 转为系统字符串
                    .map(|value| NSString::from_str(value))
                    // 收集为列表
                    .collect();
                    // 构造元数据字典
                    let metadata = NSDictionary::<NSString, NSString>::from_slices(
                        // 键列表
                        &keys.iter().map(|key| &**key).collect::<Vec<_>>(),
                        // 值列表
                        &values.iter().map(|value| &**value).collect::<Vec<_>>(),
                    );
                    // 文件 URL
                    let file = NSURL::fileURLWithPath(&NSString::from_str(
                        // 来自传输路径
                        &transfer.path.to_string_lossy(),
                    ));
                    // 交给系统传输
                    let _ = self.session.transfer_file(&file, Some(&metadata));
                    // 标记该资源有传输占用
                    occupied.insert(transfer.hash.clone());
                    // 登记进行中的传输
                    self.in_flight.insert(transfer.path.clone(), transfer);
                }
                // 暂存失败：记为本地失败并退避
                Err(_) => {
                    // 标记本地失败
                    self.local_failures.insert(asset.hash.clone());
                    // 累计尝试次数
                    self.attempts
                        // 按哈希取条目
                        .entry(asset.hash.clone())
                        // 默认策略
                        .or_default()
                        // 登记失败
                        .failed(now);
                }
            }
        }
    }

    // 取消不兼容的系统传输，保留与清单兼容的，并清理多余的暂存文件
    pub(super) fn cancel_outstanding(&mut self, manifest: Option<&Manifest>) {
        // 系统里未完成的传输
        let outstanding = self.session.outstanding();
        // 需要保留的文件路径
        let mut retained = BTreeSet::new();
        // 遍历未完成传输
        for index in 0..outstanding.count() {
            // 取出一个传输
            let transfer = outstanding.objectAtIndex(index);
            // 读取其描述
            let Some(description) = transfer.description() else {
                // 描述无效
                continue;
            };
            // 兼容：保留并登记
            if manifest.is_some_and(|manifest| compatible(&description, manifest)) {
                // 保留其路径
                retained.insert(description.path.clone());
                // 登记进行中的传输
                self.in_flight
                    // 按路径取条目
                    .entry(description.path.clone())
                    // 没有则插入
                    .or_insert(description);
            // 不兼容：取消
            } else {
                // 取消该传输
                transfer.cancel();
                // 移出进行中的传输
                self.in_flight.remove(&description.path);
            }
        }
        // 我们由系统持有的队列可能比本进程活得更久。裁剪之前先调和其文件。
        // 系统拥有的队列可能比本进程活得久；清理暂存文件前先对账
        self.in_flight
            // 只保留与清单兼容的传输
            .retain(|_, transfer| manifest.is_some_and(|manifest| compatible(transfer, manifest)));
        // 进行中的传输也要保留
        retained.extend(self.in_flight.keys().cloned());
        // 删除未被保留的暂存文件
        if let Ok(files) = fs::read_dir(self.root.join("staging")) {
            // 遍历暂存目录里的文件
            for file in files.flatten() {
                // 不在保留集合里
                if !retained.contains(&file.path()) {
                    // 删除
                    files::remove(&self.root, &file.path());
                }
            }
        }
    }
}
