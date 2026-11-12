use super::*;

impl Engine {
    // 开始保存名称：校验、先写存储，再生成重命名命令
    pub(super) fn begin_name(
        &mut self,
        draft: String,
        view: Value,
        now_ms: u64,
        store: &mut impl Store,
    ) -> Response {
        // 已有待确认的修改，忽略重复提交
        if self.pending.is_some() {
            return self.response();
        }
        let previous = Device::projected(&view);
        self.name_issue = name_error(&draft).map(str::to_owned);
        // 身份不可用也视为失败
        if previous.is_err() && self.name_issue.is_none() {
            self.name_issue = Some(SAVE_FAILED.into());
        }
        if self.name_issue.is_some() {
            self.name_feedback = Some(SAVE_FAILED.into());
            return self.response();
        }
        let previous = previous.expect("validated device");
        let attempted = Device {
            id: previous.id.clone(),
            name: draft,
        };
        // 先保存，失败则不下发命令
        if attempted.persist(store).is_err() {
            self.name_feedback = Some(SAVE_FAILED.into());
            return self.response();
        }
        let command = json!({"op":"rename", "name":attempted.name});
        // 进入待确认状态，3 秒内必须被连接投影确认
        self.pending = Some(PendingName {
            previous,
            attempted,
            deadline_ms: now_ms.saturating_add(NAME_CONFIRM_MS),
        });
        let mut result = self.response();
        result.commands.push(command);
        result
    }

    // 观察连接投影是否已确认新名称，返回需要写回输入框的名称
    pub(super) fn observe_name(
        &mut self,
        draft: &str,
        view: &Value,
        now_ms: u64,
        store: &mut impl Store,
    ) -> Option<String> {
        // 没有待确认的修改则无事可做
        let pending = self.pending.as_ref()?;
        // 由视图得到当前设备
        let current = Device::projected(view).ok();
        // 投影与尝试的身份一致：确认成功
        if current.as_ref() == Some(&pending.attempted) {
            let name = pending.attempted.name.clone();
            self.pending = None;
            // 确认旧请求时不覆盖期间继续输入的草稿、校验或反馈。
            // 草稿未再改动才清除校验并显示成功；确认旧请求时不覆盖继续输入的内容
            if draft == name || draft.trim_matches(comparison_whitespace) == name {
                self.name_issue = None;
                // 提示已保存，下次连接生效
                self.name_feedback = Some("已保存，下次连接生效".into());
                return Some(name);
            }
        // 超时或设备 ID 变了：视为失败
        } else if now_ms >= pending.deadline_ms
            // 或者当前设备与之前的不是同一个
            || current.as_ref().is_none_or(|d| d.id != pending.previous.id)
        {
            self.fail_name(store);
        }
        // 没有规范化后的名称
        None
    }

    // 名称保存失败：结束待确认并按需回滚
    pub(super) fn fail_name(&mut self, store: &mut impl Store) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        self.name_feedback = Some(SAVE_FAILED.into());
        // 另一个窗口可能已保存更新值，旧请求只能回滚自己仍占有的记录。
        // 读取当前存储的记录
        let stored = store
            .get(DEVICE)
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
        // 只有记录仍是本次尝试写入的才回滚，避免覆盖其他窗口更新的值
        if stored.as_ref() == Some(&json!(pending.attempted)) {
            // 尝试把之前的设备写回存储
            let _ = pending.previous.persist(store);
        }
    }
}
