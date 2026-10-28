use super::*;

// 测试辅助
#[cfg(test)]
impl Owner {
    /// 不读写真实凭据位置的 owner：记录使用 `service` 指定的隔离安全存储条目。
    // 不读写真实凭据位置的 owner：记录使用 service 指定的隔离条目
    pub(in crate::native::connection) fn for_test(
        // 根目录
        root: PathBuf,
        // 设备标识
        id: &str,
        // 设备名
        name: &str,
        // 凭据服务名
        service: &str,
        // 事件发送端
        events: SyncSender<Event>,
        // 返回所有者
    ) -> Self {
        // 创建私有目录
        persistence::private_dir(&root).unwrap();
        // 构造所有者
        Self {
            // 根目录
            root,
            // 设备标识
            id: id.into(),
            // 设备名
            name: name.into(),
            // 平台为 iOS
            platform: "ios".into(),
            // 启用手表配套
            watch_companion: true,
            // 默认小组件共享
            widget: crate::native::connection::widget::WidgetShare::default(),
            // 默认应用版本
            app_version: "1.0.0".into(),
            // 无设备形态
            form_factor: None,
            // 保留快照
            keep_snapshot: true,
            // 空视图
            view: empty_view(id, name),
            // 默认已发布状态
            published: Arc::new(Published::default()),
            // 需要发布
            dirty: true,
            // 事件通道
            events,
            // 未停止
            stopped: Arc::new(AtomicBool::new(false)),
            // 默认协议状态
            protocol: Protocol::default(),
            // 修订号从 0 开始
            revision: Arc::new(AtomicU64::new(0)),
            // 无配对记录
            records: BTreeMap::new(),
            // 无端点
            endpoints: BTreeMap::new(),
            // 无选中项
            selected: None,
            // 测试中不做真实发现
            discovery: Discovery::disabled(),
            // 尚无套接字
            socket: None,
            // 连接代次为 0
            generation: 0,
            // 生命周期状态
            lifecycle: Lifecycle {
                // 可见
                visible: true,
                // 默认允许
                permitted: true,
            },
            // 无配对邀请
            pairing_offer: None,
            // 无配对期限
            pairing_deadline: None,
            // 无重试时刻
            retry_at: None,
            // 重试次数为 0
            retries: 0,
            // 默认资源状态
            assets: AssetState::default(),
            // 能力集合为空
            capabilities: BTreeSet::new(),
            // 凭据服务名
            credential_service: service.into(),
            // 尚未上报设备形态
            form_factor_reported: false,
        }
    }

    /// 测试读取最近一次提交的公开投影。
    // 测试读取最近一次提交的公开投影
    pub(in crate::native::connection) fn published_view(&mut self) -> Value {
        // 提交变更
        self.commit();
        // 锁定已发布状态
        let state = self.published.state.lock().unwrap();
        // 解析当前视图
        serde_json::from_str(&state.view).unwrap()
    }
}
