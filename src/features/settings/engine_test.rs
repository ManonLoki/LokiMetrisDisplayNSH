// 引入被测引擎
use super::*;
// 引入有序映射
use std::collections::BTreeMap;

// 测试用设备 ID
const ID: &str = "11111111-1111-4111-8111-111111111111";

#[derive(Default)]
// 内存中的存储，可设置为拒绝写入
struct Memory {
    values: BTreeMap<String, String>,
    // 为 true 时所有写入失败
    reject: bool,
}

// 内存存储的实现
impl Store for Memory {
    // 按键读取
    fn get(&self, key: &str) -> Option<String> {
        // 返回副本
        self.values.get(key).cloned()
    }
    // 拒绝模式下返回配额错误
    fn set(&mut self, key: &str, value: String) -> Result<(), String> {
        if self.reject {
            return Err("quota exceeded".into());
        }
        self.values.insert(key.into(), value);
        Ok(())
    }
}

// 执行一个请求，缺少会话号时自动补当前会话
fn run(engine: &mut Engine, store: &mut Memory, mut value: Value) -> Response {
    if value["op"] != "boot" && value.get("session").is_none() {
        value["session"] = json!(engine.session);
    }
    engine
        .apply(serde_json::from_value(value).unwrap(), store)
        .unwrap()
}

// 构造最小的连接视图
fn view(name: &str) -> Value {
    json!({"deviceId":ID,"deviceName":name})
}

// 已启动的引擎，存储中已有设备身份
fn ready() -> (Engine, Memory) {
    let mut store = Memory::default();
    store.values.insert(
        DEVICE.into(),
        json!({"id":ID,"name":"客厅手机"}).to_string(),
    );
    let mut engine = Engine::default();
    run(&mut engine, &mut store, json!({"op":"boot"}));
    // 返回引擎与存储
    (engine, store)
}

// 开始保存名称“书房展示”
fn begin(engine: &mut Engine, store: &mut Memory) -> Response {
    run(
        engine,
        store,
        json!({"op":"beginNameSave","draft":"书房展示","view":view("客厅手机"),"nowMs":1000}),
    )
}

// 读取存储里保存的 JSON
fn saved(store: &Memory, key: &str) -> Value {
    serde_json::from_str(&store.values[key]).unwrap()
}

#[test]
// 启动时用原键恢复，不改写已有记录
fn boot_restores_original_keys_without_rewriting_existing_records() {
    let (mut engine, mut store) = ready();
    // 存储里已有面板偏好的原始文本
    let raw =
        r#"{"showUsage":false,"showHooks":false,"usageDetail":"detailed","hookLayout":"2x2"}"#;
    store.values.insert(PANEL.into(), raw.into());
    // 重新启动
    let result = run(&mut engine, &mut store, json!({"op":"boot"}));
    // 恢复出原有的设备身份、名称草稿与面板
    assert_eq!(result.device.unwrap().id, ID);
    assert_eq!(result.initialize.unwrap().id, ID);
    assert_eq!(result.draft.as_deref(), Some("客厅手机"));
    assert_eq!(result.panel.unwrap()["showUsage"], false);
    // 启动不会改写已有记录
    assert_eq!(store.values[PANEL], raw);
}

#[test]
// 身份损坏时请求原生端口生成，并保存其确认的身份
fn corrupt_identity_requests_native_generation_and_persists_its_confirmation() {
    let mut engine = Engine::default();
    // 存储里的身份损坏、面板配置不是合法 JSON
    let mut store = Memory::default();
    store
        .values
        .insert(DEVICE.into(), r#"{"id":"bad-id","name":"旧名称"}"#.into());
    store.values.insert(PANEL.into(), "broken JSON".into());
    let result = run(&mut engine, &mut store, json!({"op":"boot"}));
    // 要求原生端口生成身份，ID 为空
    assert_eq!(result.initialize.unwrap().id, "");
    // UI 把未确认的身份显示为不可用,而不是占位 UUID。
    // 界面把未确认的身份显示为不可用，而不是占位 UUID
    assert_eq!(result.device.unwrap().id, "");
    // 名称仍沿用
    assert_eq!(result.draft.as_deref(), Some("旧名称"));
    // 面板回落到默认值
    assert_eq!(result.panel.unwrap()["hookLayout"], "1x1");
    // 原生端口确认身份后，引擎把它保存
    run(
        &mut engine,
        &mut store,
        json!({"op":"initialized","view":view("旧名称")}),
    );
    assert_eq!(saved(&store, DEVICE), json!({"id":ID,"name":"旧名称"}));
    // 损坏的面板文本不被改写
    assert_eq!(store.values[PANEL], "broken JSON");
}

#[test]
// 只有本地没有偏好时才采纳旧版偏好
fn legacy_preferences_are_adopted_only_when_no_local_preferences_exist() {
    let (mut engine, mut store) = ready();
    // 原生视图带着旧版的面板偏好
    let mut native = view("客厅手机");
    native["panelConfig"] = json!({"showUsage":false,"hookLayout":"2x2"});
    // 本地没有偏好：采纳旧版偏好
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"initialized","view":native}),
    );
    assert_eq!(result.panel.unwrap()["showUsage"], false);
    assert_eq!(saved(&store, PANEL)["hookLayout"], "2x2");
    let existing = store.values[PANEL].clone();
    // 重新启动后，本地已有偏好
    run(&mut engine, &mut store, json!({"op":"boot"}));
    // 旧版偏好再变也不会再被采纳
    native["panelConfig"]["showUsage"] = json!(true);
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"initialized","view":native}),
    );
    assert!(result.panel.is_none());
    assert_eq!(store.values[PANEL], existing);
}

#[test]
// 面板写入被拒时，可见偏好不变，也不下发命令
fn rejected_panel_write_never_changes_visible_preferences_or_dispatches_a_command() {
    let (mut engine, mut store) = ready();
    // 存储拒绝写入
    store.reject = true;
    // 保存面板
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"savePanel","panel":{"showUsage":false}}),
    );
    // 可见偏好不变，也没有命令
    assert!(result.panel.is_none());
    assert!(result.commands.is_empty());
    // 给出保存失败的提示
    assert_eq!(
        result.panel_issue.as_deref(),
        Some("面板设置保存失败，请重试")
    );
    assert!(!store.values.contains_key(PANEL));
    // 恢复后再保存即成功
    store.reject = false;
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"savePanel","panel":{"showUsage":false}}),
    );
    assert!(result.commands.is_empty());
    assert_eq!(result.panel.unwrap(), saved(&store, PANEL));
    // 没有面板问题
    assert!(result.panel_issue.is_none());
}

#[test]
// 非法名称与被拒的存储写入都不会到达原生端口
fn illegal_names_and_rejected_storage_never_reach_the_native_port() {
    let (mut engine, mut store) = ready();
    let old = store.values[DEVICE].clone();
    // 超长名称（161 字节）
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"beginNameSave","draft":"a".repeat(161),"view":view("客厅手机"),"nowMs":1000}),
    );
    // 不下发命令，也不进入保存中
    assert!(result.commands.is_empty());
    assert!(!result.saving_name);
    assert_eq!(
        result.name_issue.as_deref(),
        Some("设备名称不能超过 160 个 UTF-8 字节")
    );
    // 存储未变
    assert_eq!(store.values[DEVICE], old);
    // 名称合法但存储拒绝写入
    store.reject = true;
    let result = begin(&mut engine, &mut store);
    // 同样不下发命令
    assert!(result.commands.is_empty());
    assert!(!result.saving_name);
    assert_eq!(result.name_feedback.as_deref(), Some(SAVE_FAILED));
    assert_eq!(store.values[DEVICE], old);
}

#[test]
// 名称先保存再下发，成功要等同一身份的确认
fn name_is_saved_before_dispatch_and_success_waits_for_the_same_identity() {
    let (mut engine, mut store) = ready();
    // 开始保存：进入保存中，存储里先写入新名称
    let result = begin(&mut engine, &mut store);
    assert!(result.saving_name);
    assert_eq!(saved(&store, DEVICE)["name"], "书房展示");
    // 并产生一条重命名命令
    assert_eq!(
        result.commands,
        vec![json!({"op":"rename","name":"书房展示"})]
    );
    assert!(result.name_feedback.is_none());
    // 连接投影仍是旧名称：继续等待
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"observeName","draft":"书房展示","view":view("客厅手机"),"nowMs":1100}),
    );
    assert!(result.saving_name);
    // 投影变成新名称：确认完成
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"observeName","draft":"书房展示","view":view("书房展示"),"nowMs":1200}),
    );
    assert!(!result.saving_name);
    assert_eq!(
        result.name_feedback.as_deref(),
        // 提示：已保存，下次连接生效
        Some("已保存，下次连接生效")
    );
    // 草稿保留为用户输入
    assert_eq!(result.draft.as_deref(), Some("书房展示"));
}

#[test]
// 确认旧请求时保留新的非法草稿及其校验提示
fn confirming_an_old_request_preserves_a_new_invalid_draft_and_its_issue() {
    let (mut engine, mut store) = ready();
    begin(&mut engine, &mut store);
    // 等待确认期间用户把草稿清空
    run(&mut engine, &mut store, json!({"op":"editName","draft":""}));
    // 此时收到旧请求的确认
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"observeName","draft":"","view":view("书房展示"),"nowMs":1200}),
    );
    assert!(!result.saving_name);
    // 不覆盖用户的新草稿
    assert!(result.draft.is_none());
    assert!(result.name_feedback.is_none());
    // 新草稿的校验提示保留
    assert_eq!(result.name_issue.as_deref(), Some("请输入设备名称"));
}

#[test]
// 投影不变时仍会超时并回滚记录
fn an_unchanged_projection_still_times_out_and_rolls_back_the_record() {
    let (mut engine, mut store) = ready();
    begin(&mut engine, &mut store);
    // 截止前 1 毫秒：连接投影仍是旧名称，继续等待
    let before = run(
        &mut engine,
        &mut store,
        json!({"op":"observeName","draft":"书房展示","view":view("客厅手机"),"nowMs":3999}),
    );
    assert!(before.saving_name);
    // 恰到截止时刻仍无变化：超时
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"observeName","draft":"书房展示","view":view("客厅手机"),"nowMs":4000}),
    );
    assert!(!result.saving_name);
    assert_eq!(result.name_feedback.as_deref(), Some(SAVE_FAILED));
    // 记录回滚到旧名称
    assert_eq!(saved(&store, DEVICE)["name"], "客厅手机");
}

#[test]
// 恰在截止时刻到达的确认先于超时被处理
fn native_confirmation_on_the_deadline_is_observed_before_expiry() {
    let (mut engine, mut store) = ready();
    begin(&mut engine, &mut store);
    // 恰在截止时刻到达了新名称的确认
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"observeName","draft":"书房展示","view":view("书房展示"),"nowMs":4000}),
    );
    // 确认优先于超时被处理
    assert_eq!(
        result.name_feedback.as_deref(),
        Some("已保存，下次连接生效")
    );
    assert_eq!(saved(&store, DEVICE)["name"], "书房展示");
}

#[test]
// 下发失败只回滚属于该请求的记录
fn dispatch_failure_rolls_back_only_the_record_owned_by_that_request() {
    let (mut engine, mut store) = ready();
    begin(&mut engine, &mut store);
    // 命令投递失败：回滚到旧名称
    run(&mut engine, &mut store, json!({"op":"nameDispatchFailed"}));
    assert_eq!(saved(&store, DEVICE)["name"], "客厅手机");
    // 再试一次，这时另一个窗口已写入了更新的名称
    begin(&mut engine, &mut store);
    store.values.insert(
        DEVICE.into(),
        json!({"id":ID,"name":"另一个窗口已保存"}).to_string(),
    );
    // 投递失败只回滚属于自己的记录
    run(&mut engine, &mut store, json!({"op":"nameDispatchFailed"}));
    // 别人的记录不动
    assert_eq!(saved(&store, DEVICE)["name"], "另一个窗口已保存");
}

#[test]
// 已释放或被替换的旧会话不能回滚新会话
fn a_disposed_or_replaced_generation_cannot_roll_back_a_new_session() {
    let (mut engine, mut store) = ready();
    begin(&mut engine, &mut store);
    // 记下当前会话号
    let old = engine.session;
    // 释放会话
    run(&mut engine, &mut store, json!({"op":"dispose"}));
    // 旧会话迟到的失败通知
    let late = serde_json::from_value(json!({"op":"nameDispatchFailed","session":old})).unwrap();
    // 被拒绝，不能回滚
    assert!(engine.apply(late, &mut store).is_err());
    assert_eq!(saved(&store, DEVICE)["name"], "书房展示");
    // 重新启动得到新会话
    run(&mut engine, &mut store, json!({"op":"boot"}));
    assert_ne!(engine.session, old);
    // 旧会话的通知依然被拒绝
    let late = serde_json::from_value(json!({"op":"nameDispatchFailed","session":old})).unwrap();
    assert!(engine.apply(late, &mut store).is_err());
    assert_eq!(saved(&store, DEVICE)["name"], "书房展示");
}

#[test]
// 原生轮询失败会结束待确认状态并恢复先前的名称
fn a_failed_native_poll_ends_pending_confirmation_and_restores_the_previous_name() {
    let (mut engine, mut store) = ready();
    begin(&mut engine, &mut store);
    // 原生轮询失败
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"portFailure","view":view("客厅手机"),"stage":"poll","error":"owner stopped"}),
    );
    // 待确认状态结束
    assert!(!result.saving_name);
    assert_eq!(result.name_feedback.as_deref(), Some(SAVE_FAILED));
    // 名称恢复为先前的
    assert_eq!(saved(&store, DEVICE)["name"], "客厅手机");
}

#[test]
// 端口失败时保留最后一份快照
fn port_failure_keeps_the_last_snapshot() {
    let (mut engine, mut store) = ready();
    let mut native = view("客厅手机");
    native["snapshot"] = json!({"public":"cached data"});
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"portFailure","view":native,"stage":"poll","error":"closed socket"}),
    );
    // 取出失败时的视图
    let failed = result.view.unwrap();
    // 状态为同步失败
    assert_eq!(failed["status"], "同步失败");
    // 未连接
    assert_eq!(failed["connected"], false);
    // 快照沿用原生快照
    assert_eq!(failed["snapshot"], native["snapshot"]);
}
