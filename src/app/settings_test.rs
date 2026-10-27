use super::*;
use std::collections::BTreeMap;

const ID: &str = "11111111-1111-4111-8111-111111111111";

#[derive(Default)]
struct Memory {
    values: BTreeMap<String, String>,
    reject: bool,
}

impl Store for Memory {
    fn get(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }
    fn set(&mut self, key: &str, value: String) -> Result<(), String> {
        if self.reject {
            return Err("quota exceeded".into());
        }
        self.values.insert(key.into(), value);
        Ok(())
    }
}

fn run(engine: &mut Engine, store: &mut Memory, mut value: Value) -> Response {
    if value["op"] != "boot" && value.get("session").is_none() {
        value["session"] = json!(engine.session);
    }
    engine
        .apply(serde_json::from_value(value).unwrap(), store)
        .unwrap()
}

fn view(name: &str) -> Value {
    json!({"deviceId":ID,"deviceName":name})
}

fn ready() -> (Engine, Memory) {
    let mut store = Memory::default();
    store.values.insert(
        DEVICE.into(),
        json!({"id":ID,"name":"客厅手机"}).to_string(),
    );
    let mut engine = Engine::default();
    run(&mut engine, &mut store, json!({"op":"boot"}));
    (engine, store)
}

fn begin(engine: &mut Engine, store: &mut Memory) -> Response {
    run(
        engine,
        store,
        json!({"op":"beginNameSave","draft":"书房展示","view":view("客厅手机"),"nowMs":1000}),
    )
}

fn saved(store: &Memory, key: &str) -> Value {
    serde_json::from_str(&store.values[key]).unwrap()
}

#[test]
fn boot_restores_original_keys_without_rewriting_existing_records() {
    let (mut engine, mut store) = ready();
    let raw =
        r#"{"showUsage":false,"showHooks":false,"usageDetail":"detailed","hookLayout":"2x2"}"#;
    store.values.insert(PANEL.into(), raw.into());
    let result = run(&mut engine, &mut store, json!({"op":"boot"}));
    assert_eq!(result.device.unwrap().id, ID);
    assert_eq!(result.initialize.unwrap().id, ID);
    assert_eq!(result.draft.as_deref(), Some("客厅手机"));
    assert_eq!(result.panel.unwrap()["showUsage"], false);
    assert_eq!(store.values[PANEL], raw);
}

#[test]
fn corrupt_identity_requests_native_generation_and_persists_its_confirmation() {
    let mut engine = Engine::default();
    let mut store = Memory::default();
    store
        .values
        .insert(DEVICE.into(), r#"{"id":"bad-id","name":"旧名称"}"#.into());
    store.values.insert(PANEL.into(), "broken JSON".into());
    let result = run(&mut engine, &mut store, json!({"op":"boot"}));
    assert_eq!(result.initialize.unwrap().id, "");
    assert_eq!(result.draft.as_deref(), Some("旧名称"));
    assert_eq!(result.panel.unwrap()["hookLayout"], "1x1");
    run(
        &mut engine,
        &mut store,
        json!({"op":"initialized","view":view("旧名称")}),
    );
    assert_eq!(saved(&store, DEVICE), json!({"id":ID,"name":"旧名称"}));
    assert_eq!(store.values[PANEL], "broken JSON");
}

#[test]
fn legacy_preferences_are_adopted_only_when_no_local_preferences_exist() {
    let (mut engine, mut store) = ready();
    let mut native = view("客厅手机");
    native["panelConfig"] = json!({"showUsage":false,"hookLayout":"2x2"});
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"initialized","view":native}),
    );
    assert_eq!(result.panel.unwrap()["showUsage"], false);
    assert_eq!(saved(&store, PANEL)["hookLayout"], "2x2");
    let existing = store.values[PANEL].clone();
    run(&mut engine, &mut store, json!({"op":"boot"}));
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
fn rejected_panel_write_never_changes_visible_preferences_or_dispatches_a_command() {
    let (mut engine, mut store) = ready();
    store.reject = true;
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"savePanel","panel":{"showUsage":false}}),
    );
    assert!(result.panel.is_none());
    assert!(result.commands.is_empty());
    assert_eq!(
        result.panel_issue.as_deref(),
        Some("面板设置保存失败，请重试")
    );
    assert!(!store.values.contains_key(PANEL));
    store.reject = false;
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"savePanel","panel":{"showUsage":false}}),
    );
    assert!(result.commands.is_empty());
    assert_eq!(result.panel.unwrap(), saved(&store, PANEL));
    assert!(result.panel_issue.is_none());
}

#[test]
fn illegal_names_and_rejected_storage_never_reach_the_native_port() {
    let (mut engine, mut store) = ready();
    let old = store.values[DEVICE].clone();
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"beginNameSave","draft":"a".repeat(161),"view":view("客厅手机"),"nowMs":1000}),
    );
    assert!(result.commands.is_empty());
    assert!(!result.saving_name);
    assert_eq!(
        result.name_issue.as_deref(),
        Some("设备名称不能超过 160 个 UTF-8 字节")
    );
    assert_eq!(store.values[DEVICE], old);
    store.reject = true;
    let result = begin(&mut engine, &mut store);
    assert!(result.commands.is_empty());
    assert!(!result.saving_name);
    assert_eq!(result.name_feedback.as_deref(), Some(SAVE_FAILED));
    assert_eq!(store.values[DEVICE], old);
}

#[test]
fn name_is_saved_before_dispatch_and_success_waits_for_the_same_identity() {
    let (mut engine, mut store) = ready();
    let result = begin(&mut engine, &mut store);
    assert!(result.saving_name);
    assert_eq!(saved(&store, DEVICE)["name"], "书房展示");
    assert_eq!(
        result.commands,
        vec![json!({"op":"rename","name":"书房展示"})]
    );
    assert!(result.name_feedback.is_none());
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"observeName","draft":"书房展示","view":view("客厅手机"),"nowMs":1100}),
    );
    assert!(result.saving_name);
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"observeName","draft":"书房展示","view":view("书房展示"),"nowMs":1200}),
    );
    assert!(!result.saving_name);
    assert_eq!(
        result.name_feedback.as_deref(),
        Some("已保存，下次连接生效")
    );
    assert_eq!(result.draft.as_deref(), Some("书房展示"));
}

#[test]
fn confirming_an_old_request_preserves_a_new_invalid_draft_and_its_issue() {
    let (mut engine, mut store) = ready();
    begin(&mut engine, &mut store);
    run(&mut engine, &mut store, json!({"op":"editName","draft":""}));
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"observeName","draft":"","view":view("书房展示"),"nowMs":1200}),
    );
    assert!(!result.saving_name);
    assert!(result.draft.is_none());
    assert!(result.name_feedback.is_none());
    assert_eq!(result.name_issue.as_deref(), Some("请输入设备名称"));
}

#[test]
fn an_unchanged_projection_still_times_out_and_rolls_back_the_record() {
    let (mut engine, mut store) = ready();
    begin(&mut engine, &mut store);
    let before = run(
        &mut engine,
        &mut store,
        json!({"op":"observeName","draft":"书房展示","view":view("客厅手机"),"nowMs":3999}),
    );
    assert!(before.saving_name);
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"observeName","draft":"书房展示","view":view("客厅手机"),"nowMs":4000}),
    );
    assert!(!result.saving_name);
    assert_eq!(result.name_feedback.as_deref(), Some(SAVE_FAILED));
    assert_eq!(saved(&store, DEVICE)["name"], "客厅手机");
}

#[test]
fn native_confirmation_on_the_deadline_is_observed_before_expiry() {
    let (mut engine, mut store) = ready();
    begin(&mut engine, &mut store);
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"observeName","draft":"书房展示","view":view("书房展示"),"nowMs":4000}),
    );
    assert_eq!(
        result.name_feedback.as_deref(),
        Some("已保存，下次连接生效")
    );
    assert_eq!(saved(&store, DEVICE)["name"], "书房展示");
}

#[test]
fn dispatch_failure_rolls_back_only_the_record_owned_by_that_request() {
    let (mut engine, mut store) = ready();
    begin(&mut engine, &mut store);
    run(&mut engine, &mut store, json!({"op":"nameDispatchFailed"}));
    assert_eq!(saved(&store, DEVICE)["name"], "客厅手机");
    begin(&mut engine, &mut store);
    store.values.insert(
        DEVICE.into(),
        json!({"id":ID,"name":"另一个窗口已保存"}).to_string(),
    );
    run(&mut engine, &mut store, json!({"op":"nameDispatchFailed"}));
    assert_eq!(saved(&store, DEVICE)["name"], "另一个窗口已保存");
}

#[test]
fn a_disposed_or_replaced_generation_cannot_roll_back_a_new_session() {
    let (mut engine, mut store) = ready();
    begin(&mut engine, &mut store);
    let old = engine.session;
    run(&mut engine, &mut store, json!({"op":"dispose"}));
    let late = serde_json::from_value(json!({"op":"nameDispatchFailed","session":old})).unwrap();
    assert!(engine.apply(late, &mut store).is_err());
    assert_eq!(saved(&store, DEVICE)["name"], "书房展示");
    run(&mut engine, &mut store, json!({"op":"boot"}));
    assert_ne!(engine.session, old);
    let late = serde_json::from_value(json!({"op":"nameDispatchFailed","session":old})).unwrap();
    assert!(engine.apply(late, &mut store).is_err());
    assert_eq!(saved(&store, DEVICE)["name"], "书房展示");
}

#[test]
fn a_failed_native_poll_ends_pending_confirmation_and_restores_the_previous_name() {
    let (mut engine, mut store) = ready();
    begin(&mut engine, &mut store);
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"portFailure","view":view("客厅手机"),"stage":"poll","error":"owner stopped"}),
    );
    assert!(!result.saving_name);
    assert_eq!(result.name_feedback.as_deref(), Some(SAVE_FAILED));
    assert_eq!(saved(&store, DEVICE)["name"], "客厅手机");
}

#[test]
fn port_failure_keeps_the_last_snapshot() {
    let (mut engine, mut store) = ready();
    let mut native = view("客厅手机");
    native["snapshot"] = json!({"public":"cached data"});
    let result = run(
        &mut engine,
        &mut store,
        json!({"op":"portFailure","view":native,"stage":"poll","error":"closed socket"}),
    );
    let failed = result.view.unwrap();
    assert_eq!(failed["status"], "同步失败");
    assert_eq!(failed["connected"], false);
    assert_eq!(failed["snapshot"], native["snapshot"]);
}
