//! 用生产WC终态转换入口验证真回执；OS预检、文件投递和冷缓存不会调用这个入口。
use super::*;
use serde_json::{Value, json};
use std::sync::Mutex;

const SOURCE: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const WATCH: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const STREAM: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const FOREGROUND: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";

/// 当前LAN探测意图将生产回调输出存入隔离收集器。
fn binding(scope: &str, reports: &Arc<Mutex<Vec<WireReport>>>) -> Command {
    let reports = reports.clone();
    Command::Probe(ProbeRequest {
        root: PathBuf::from("/isolated/companion-v1"),
        source_id: SOURCE.into(),
        scope: scope.into(),
        sink: Arc::new(move |report| reports.lock().unwrap().push(report)),
    })
}
/// manifest身份与生产Watch探针完全相同。
fn expected() -> ExpectedSnapshot {
    ExpectedSnapshot {
        stream_id: STREAM.into(),
        revision: "1".into(),
        source_id: SOURCE.into(),
    }
}
/// 构造系统回调所复制的真实reply JSON形状。
fn reply(challenge: &str, sync: bool) -> Value {
    json!({"version":1,"type":"presenceReply","challenge":challenge,
        "info":{"deviceId":WATCH,"name":"Apple Watch","appVersion":"1.0.15"},
        "active":true,"foregroundId":FOREGROUND,
        "sync":if sync { json!({"streamId":STREAM,"revision":"1","sourceId":SOURCE,"ageMs":20}) } else { Value::Null }})
}
/// 将JSON字节交给生产终态入口，不创建替代的在线状态计算器。
fn complete(controller: &mut Controller, challenge: &str, value: Value, now: Instant) {
    controller.complete(challenge, Some(&serde_json::to_vec(&value).unwrap()), now);
}

#[test]
fn foreground_callback_produces_strict_wire_and_duplicate_snapshot_does_not_resync() {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let mut controller = Controller::default();
    let at = Instant::now();
    controller.command(binding("one", &reports));
    let (challenge, request) = controller.start(expected(), at).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&request).unwrap()["type"],
        "presenceProbe"
    );
    assert!(
        reports.lock().unwrap().is_empty(),
        "发送探针本身不是在线证明"
    );
    complete(
        &mut controller,
        &challenge,
        reply(&challenge, true),
        at + Duration::from_micros(1001),
    );
    let first = reports.lock().unwrap()[0].clone();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        json!({
        "info":{"deviceId":WATCH,"name":"Apple Watch","appVersion":"1.0.15"},
        "reachable":true,"contact":{"sequence":1,"ageMs":2},"sync":{"sequence":1,"ageMs":22}})
    );
    controller.command(binding("one", &reports));
    let (next, _) = controller
        .start(expected(), at + Duration::from_secs(1))
        .unwrap();
    complete(
        &mut controller,
        &next,
        reply(&next, true),
        at + Duration::from_millis(1002),
    );
    let second = reports.lock().unwrap().last().unwrap().clone();
    assert_eq!(second.contact.unwrap().sequence, 2);
    assert_eq!(
        second.sync.unwrap().sequence,
        1,
        "重复probe不能刷新同步序号"
    );
    assert!(second.sync.unwrap().age_ms > first.sync.unwrap().age_ms);
}

#[test]
fn os_state_and_failed_or_wrong_challenge_callbacks_never_create_presence() {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let mut controller = Controller::default();
    let at = Instant::now();
    controller.command(binding("one", &reports));
    controller.unavailable(at);
    assert!(reports.lock().unwrap().is_empty());
    controller.command(binding("one", &reports));
    let (challenge, _) = controller.start(expected(), at).unwrap();
    complete(
        &mut controller,
        "wrong-terminal",
        reply(&challenge, true),
        at,
    );
    assert!(controller.pending.is_some(), "其它回调不能释放当前物理槽");
    let mut invalid = reply(&challenge, true);
    invalid["challenge"] = json!(uuid::Uuid::new_v4().to_string());
    complete(&mut controller, &challenge, invalid, at);
    assert!(reports.lock().unwrap().is_empty());
    controller.command(binding("one", &reports));
    let (challenge, _) = controller.start(expected(), at).unwrap();
    let mut invalid = reply(&challenge, true);
    invalid["sync"]["sourceId"] = json!(WATCH);
    complete(&mut controller, &challenge, invalid, at);
    assert!(reports.lock().unwrap().is_empty());
}

#[test]
fn foreground_revocation_blocks_late_positive_but_not_new_foreground() {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let mut controller = Controller::default();
    let at = Instant::now();
    controller.command(binding("one", &reports));
    let (challenge, _) = controller.start(expected(), at).unwrap();
    let revoke = serde_json::to_vec(
        &json!({"version":1,"type":"presenceRevoked","deviceId":WATCH,"foregroundId":FOREGROUND}),
    )
    .unwrap();
    controller.revoke(&revoke, at);
    complete(&mut controller, &challenge, reply(&challenge, true), at);
    assert!(
        reports.lock().unwrap().is_empty(),
        "撤销先于迟到reply也不能复活旧代次"
    );
    controller.command(binding("one", &reports));
    let (next, _) = controller.start(expected(), at).unwrap();
    let mut value = reply(&next, true);
    value["foregroundId"] = json!(uuid::Uuid::new_v4().to_string());
    complete(&mut controller, &next, value, at);
    assert!(reports.lock().unwrap().last().unwrap().reachable);
    controller.revoke(&revoke, at);
    assert!(
        reports.lock().unwrap().last().unwrap().reachable,
        "旧前台撤销不影响新代次"
    );
}

#[test]
fn timeout_and_lan_replacement_discard_late_callback_without_reusing_physical_slot() {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let mut controller = Controller::default();
    let at = Instant::now();
    controller.command(binding("one", &reports));
    let (challenge, _) = controller.start(expected(), at).unwrap();
    controller.expire(at + PROBE_TIMEOUT);
    controller.command(binding("two", &reports));
    assert!(
        controller.start(expected(), at + PROBE_TIMEOUT).is_none(),
        "超时不能叠加物理请求"
    );
    complete(
        &mut controller,
        &challenge,
        reply(&challenge, true),
        at + PROBE_TIMEOUT,
    );
    assert!(reports.lock().unwrap().is_empty());
    let (next, _) = controller.start(expected(), at + PROBE_TIMEOUT).unwrap();
    complete(
        &mut controller,
        &next,
        reply(&next, false),
        at + PROBE_TIMEOUT,
    );
    assert_eq!(
        reports
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .contact
            .unwrap()
            .sequence,
        1
    );
    controller.command(Command::Cancel("two".into()));
    controller.command(binding("three", &reports));
    let (next, _) = controller.start(expected(), at + PROBE_TIMEOUT).unwrap();
    complete(
        &mut controller,
        &next,
        reply(&next, false),
        at + PROBE_TIMEOUT,
    );
    assert_eq!(
        reports
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .contact
            .unwrap()
            .sequence,
        2,
        "LAN重连不复位owner内真实回执高水位"
    );
}

#[test]
fn inactive_reply_cannot_smuggle_sync_or_revive_previous_foreground() {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let mut controller = Controller::default();
    let at = Instant::now();
    controller.command(binding("one", &reports));
    let (challenge, _) = controller.start(expected(), at).unwrap();
    let mut value = reply(&challenge, true);
    value["active"] = json!(false);
    complete(&mut controller, &challenge, value, at);
    assert!(reports.lock().unwrap().is_empty());
    controller.command(binding("one", &reports));
    let (next, _) = controller.start(expected(), at).unwrap();
    let mut value = reply(&next, false);
    value["active"] = json!(false);
    value["foregroundId"] = Value::Null;
    complete(&mut controller, &next, value, at);
    let report = reports.lock().unwrap().last().unwrap().clone();
    assert!(!report.reachable);
    assert!(report.sync.is_none());
    assert_eq!(report.contact.unwrap().sequence, 1);
}

/// 撤销邮箱满时即使当前代次那条撤销未被保留，也必须先失效物理请求的逻辑资格。
#[test]
fn revocation_overflow_blocks_the_unretained_foregrounds_late_reply() {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let mut controller = Controller::default();
    let at = Instant::now();
    controller.command(binding("one", &reports));
    let (first, _) = controller.start(expected(), at).unwrap();
    complete(&mut controller, &first, reply(&first, true), at);
    controller.command(binding("one", &reports));
    let (pending, _) = controller.start(expected(), at).unwrap();
    let retained = (0..16)
        .map(|index| {
            serde_json::to_vec(&json!({
                "version": 1, "type": "presenceRevoked", "deviceId": WATCH,
                "foregroundId": format!("00000000-0000-4000-8000-{index:012}")
            }))
            .unwrap()
        })
        .collect();
    controller.revoke_batch(retained, true, at);
    assert!(
        controller.pending.as_ref().unwrap().expired,
        "保留物理槽但撤销迟到回复资格"
    );
    assert!(!reports.lock().unwrap().last().unwrap().reachable);
    complete(&mut controller, &pending, reply(&pending, true), at);
    assert!(controller.pending.is_none());
    let last = reports.lock().unwrap().last().unwrap().clone();
    assert!(!last.reachable, "第17个撤销不能因邮箱截断而让旧前台复活");
    assert_eq!(last.contact.unwrap().sequence, 1);
}

/// null是协议字段的有效值，省略字段不是同一种报文，不能形成任何接触证明。
#[test]
fn reply_requires_both_nullable_fields_to_be_present() {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let mut controller = Controller::default();
    let at = Instant::now();
    for missing in ["foregroundId", "sync"] {
        controller.command(binding("one", &reports));
        let (challenge, _) = controller.start(expected(), at).unwrap();
        let mut invalid = reply(&challenge, false);
        invalid["active"] = json!(false);
        invalid["foregroundId"] = Value::Null;
        invalid.as_object_mut().unwrap().remove(missing);
        complete(&mut controller, &challenge, invalid, at);
        assert!(
            reports.lock().unwrap().is_empty(),
            "缺失{missing}不能更新序号"
        );
    }
}

/// 新前台探针只能更新真实接触，未接纳新快照时保留历史同步时间且不续租。
#[test]
fn next_foreground_contact_does_not_refresh_previous_sync() {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let mut controller = Controller::default();
    let at = Instant::now();
    controller.command(binding("one", &reports));
    let (first, _) = controller.start(expected(), at).unwrap();
    complete(&mut controller, &first, reply(&first, true), at);
    controller.command(binding("one", &reports));
    let (next, _) = controller
        .start(expected(), at + Duration::from_secs(1))
        .unwrap();
    let mut value = reply(&next, false);
    value["foregroundId"] = json!(uuid::Uuid::new_v4().to_string());
    complete(&mut controller, &next, value, at + Duration::from_secs(1));
    let last = reports.lock().unwrap().last().unwrap().clone();
    assert_eq!(last.contact.unwrap().sequence, 2);
    assert_eq!(last.sync.unwrap().sequence, 1);
    assert!(last.sync.unwrap().age_ms >= 1000);
}
