use super::*;

#[cfg(test)]
impl Owner {
    /// 不读写真实凭据位置的 owner：记录使用 `service` 指定的隔离安全存储条目。
    // 不读写真实凭据位置的 owner：记录使用 service 指定的隔离条目
    pub(in crate::native::connection) fn for_test(
        root: PathBuf,
        id: &str,
        name: &str,
        service: &str,
        events: SyncSender<Event>,
    ) -> Self {
        persistence::private_dir(&root).unwrap();
        Self {
            root,
            id: id.into(),
            name: name.into(),
            platform: "ios".into(),
            watch_companion: true,
            widget: crate::native::connection::widget::WidgetShare::default(),
            app_version: "1.0.0".into(),
            form_factor: None,
            keep_snapshot: true,
            view: empty_view(id, name),
            published: Arc::new(Published::default()),
            dirty: true,
            events,
            stopped: Arc::new(AtomicBool::new(false)),
            protocol: Protocol::default(),
            // 修订号从 0 开始
            revision: Arc::new(AtomicU64::new(0)),
            records: BTreeMap::new(),
            endpoints: BTreeMap::new(),
            selected: None,
            // 测试中不做真实发现
            discovery: Discovery::disabled(),
            socket: None,
            generation: 0,
            lifecycle: Lifecycle {
                visible: true,
                permitted: true,
            },
            pairing_offer: None,
            pairing_deadline: None,
            retry_at: None,
            retries: 0,
            assets: AssetState::default(),
            capabilities: BTreeSet::new(),
            credential_service: service.into(),
            form_factor_reported: false,
            watch: Default::default(),
        }
    }

    /// 测试读取最近一次提交的公开投影。
    // 测试读取最近一次提交的公开投影
    pub(in crate::native::connection) fn published_view(&mut self) -> Value {
        self.commit();
        // 锁定已发布状态
        let state = self.published.state.lock().unwrap();
        serde_json::from_str(&state.view).unwrap()
    }
}
