use super::*;

impl Actor {
    // 推进同步：激活会话、发送上下文、传输缺失的图片
    pub(super) fn flush(&mut self) {
        let now = Instant::now();
        if self
            .activation_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.activation_deadline = None;
            self.activation_retry.failed(now);
        }
        // 未激活：仅在未激活（0）状态才发起激活；不活跃（1）需先完成停用，才能为另一块手表重新激活
        if self.session.activation_state() != 2 {
            // Inactive (1) 必须先完成停用,才能为另一只手表重新激活。
            if self.session.activation_state() == 0
                && self.activation_deadline.is_none()
                && self.activation_retry.ready(now)
            {
                self.activation_retry.started();
                // 设 15 秒的激活期限
                self.activation_deadline = Some(now + Duration::from_secs(15));
                self.session.activate();
            }
            return;
        }
        // 没有配对手表或手表应用未安装：取消所有传输并清除上下文
        if !self.session.paired() || !self.session.installed() {
            // 取消所有进行中的传输
            self.cancel_outstanding(None);
            self.context = None;
            return;
        }
        // 清单不可读则等待下次
        let Ok(manifest) = read_manifest(&self.root) else {
            return;
        };
        let switched = self.manifest.as_ref().is_some_and(|previous| {
            // 流或来源与之前不同
            previous.stream_id != manifest.stream_id || previous.source_id != manifest.source_id
        });
        // 切换来源或清空：取消全部传输并清空状态
        if switched || manifest.snapshot.is_none() {
            // 取消所有进行中的传输
            self.cancel_outstanding(None);
            self.successful.clear();
            self.attempts.clear();
            self.local_failures.clear();
        // 否则只取消与新清单不兼容的传输
        } else {
            // 只取消不在新清单里的传输
            self.cancel_outstanding(Some(&manifest));
        }
        // 清单变化：重发上下文，并裁剪不再引用的资源状态
        if self.manifest.as_ref() != Some(&manifest) {
            self.context = None;
            self.context_retry = Retry::default();
            self.successful
                // 只保留清单里仍有的哈希
                .retain(|hash| manifest.assets.iter().any(|asset| &asset.hash == hash));
            self.attempts
                // 只保留清单里仍有的传输
                .retain(|hash, _| manifest.assets.iter().any(|asset| &asset.hash == hash));
            self.local_failures
                // 只保留仍在清单里的资源
                .retain(|hash| manifest.assets.iter().any(|asset| &asset.hash == hash));
            self.manifest = Some(manifest.clone());
        }
        // 发送应用上下文（不超过 48 KiB）；失败则退避
        if self.context.is_none() && self.context_retry.ready(now) {
            // 记录上下文发送开始
            self.context_retry.started();
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
        let mut occupied: BTreeSet<_> = self
            .in_flight
            .values()
            .map(|transfer| transfer.hash.clone())
            .collect();
        // 逐个资源安排传输，最多同时 MAX_TRANSFERS 个
        for asset in &manifest.assets {
            // 进行中的传输已达上限
            if self.in_flight.len() >= MAX_TRANSFERS {
                break;
            }
            // 已成功、传输中或尚在退避的跳过
            if self.successful.contains(&asset.hash)
                // 或者该哈希已被占用
                || occupied.contains(&asset.hash)
                || !self
                    .attempts
                    .entry(asset.hash.clone())
                    .or_default()
                    .ready(now)
            {
                continue;
            }
            // 记录开始一次尝试
            self.attempts
                .entry(asset.hash.clone())
                .or_default()
                .started();
            match files::stage(&self.root, asset) {
                Ok(path) => {
                    self.local_failures.remove(&asset.hash);
                    // 暂存可能耗时;所有者上的来源切换必须优先。
                    // 暂存可能耗时；期间 owner 切换了来源就丢弃，以新状态为准
                    if !read_manifest(&self.root).is_ok_and(|latest| {
                        // 最新清单仍属于同一个流
                        latest.stream_id == manifest.stream_id
                            && latest.source_id == manifest.source_id
                            && latest.assets.iter().any(|candidate| candidate == asset)
                    }) {
                        // 否则删除这份暂存文件
                        files::remove(&self.root, &path);
                        wake();
                        return;
                    }
                    let transfer = Transfer {
                        stream: manifest.stream_id.clone(),
                        revision: manifest.revision.parse().unwrap_or(0),
                        hash: asset.hash.clone(),
                        path,
                    };
                    // 传输元数据：类型、流 ID、修订号与摘要
                    let keys: Vec<_> = ["kind", "streamId", "revision", "hash"]
                        .iter()
                        .map(|key| NSString::from_str(key))
                        .collect();
                    let values: Vec<_> = [
                        "asset",
                        &manifest.stream_id,
                        &manifest.revision,
                        &asset.hash,
                    ]
                    .iter()
                    .map(|value| NSString::from_str(value))
                    .collect();
                    let metadata = NSDictionary::<NSString, NSString>::from_slices(
                        &keys.iter().map(|key| &**key).collect::<Vec<_>>(),
                        &values.iter().map(|value| &**value).collect::<Vec<_>>(),
                    );
                    let file = NSURL::fileURLWithPath(&NSString::from_str(
                        &transfer.path.to_string_lossy(),
                    ));
                    let _ = self.session.transfer_file(&file, Some(&metadata));
                    // 标记该资源有传输占用
                    occupied.insert(transfer.hash.clone());
                    self.in_flight.insert(transfer.path.clone(), transfer);
                }
                // 暂存失败：记为本地失败并退避
                Err(_) => {
                    self.local_failures.insert(asset.hash.clone());
                    self.attempts
                        .entry(asset.hash.clone())
                        .or_default()
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
        for index in 0..outstanding.count() {
            let transfer = outstanding.objectAtIndex(index);
            let Some(description) = transfer.description() else {
                continue;
            };
            if manifest.is_some_and(|manifest| compatible(&description, manifest)) {
                retained.insert(description.path.clone());
                self.in_flight
                    .entry(description.path.clone())
                    .or_insert(description);
            } else {
                transfer.cancel();
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
                if !retained.contains(&file.path()) {
                    files::remove(&self.root, &file.path());
                }
            }
        }
    }
}
