// 引入句柄与事件
use super::super::{Command, Event, Handle};
use super::*;
// 引入配对库，用来模拟发送端
use loki_metris_pairing::{AwaitingAck, SenderSession};
use std::{
    sync::{atomic::Ordering, mpsc},
    time::Duration,
};

// 测试用的各个 ID
const CLIENT: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const OTHER: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const DEVICE: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const SESSION: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";

// 构造一次待提交的配对
fn pending(now: Instant) -> (Protocol, AwaitingAck, Commit) {
    // 以“本轮名称”和配对码 0037 开窗
    let mut protocol = Protocol::default();
    protocol
        .start_pairing(CLIENT, "本轮名称", "0037".into(), 0, now)
        .unwrap();
    // 模拟发送端用正确的码发起
    let (sender, hello) = SenderSession::start(CLIENT, DEVICE, "0037", SESSION.into()).unwrap();
    let reply = protocol.hello(hello, DEVICE, now).unwrap();
    let (sender, confirmation) = sender.accept(reply).unwrap();
    // 得到待提交的保存资格
    let commit = protocol.confirmation(confirmation, now).unwrap();
    (protocol, sender, commit)
}

// 构造句柄与事件接收端
fn handle(capacity: usize) -> (Handle, mpsc::Receiver<Event>) {
    Handle::for_test(capacity, Some(CLIENT))
}

#[test]
// 配对 transcript 使用开窗时的名称
fn pairing_transcript_keeps_the_name_that_opened_the_window() {
    let now = Instant::now();
    let mut protocol = Protocol::default();
    // 界面上显示的名称
    let mut displayed_name = "本轮名称".to_owned();
    protocol
        // 开窗时使用当时的名称
        .start_pairing(CLIENT, &displayed_name, "0037".into(), 0, now)
        .unwrap();
    // 随后用户编辑了名称
    displayed_name = "随后编辑的新名称".into();
    let (sender, hello) = SenderSession::start(CLIENT, DEVICE, "0037", SESSION.into()).unwrap();
    let reply = protocol.hello(hello, DEVICE, now).unwrap();
    // 回复里仍是开窗时的名称
    assert_eq!(reply.name, "本轮名称");
    assert_ne!(reply.name, displayed_name);
    let (sender, confirmation) = sender.accept(reply).unwrap();
    let commit = protocol.confirmation(confirmation, now).unwrap();
    // 保存材料同样使用开窗时的名称
    assert_eq!(commit.material().device_name(), "本轮名称");
    let ack = commit.finish(&mut protocol, 0, now).unwrap();
    // 发送端验证到的名称也一致
    assert_eq!(sender.verify(ack).unwrap().device_name(), "本轮名称");
}

#[test]
// 已排队的取消会在 owner 处理队列前就撤销 ACK
fn queued_cancel_revokes_ack_before_the_owner_can_drain_its_commands() {
    let now = Instant::now();
    let (mut protocol, _, commit) = pending(now);
    // 保存资格此刻有效
    commit.check(&mut protocol, 0, now).unwrap();
    let (handle, rx) = handle(1);
    // 模拟 OS 保存阻塞期间 UI 提交取消；队列仍未由 owner 消费。
    // 模拟 OS 保存阻塞期间 UI 提交了取消；命令仍在队列里，owner 尚未消费
    handle.command(r#"{"op":"active","value":false}"#).unwrap();
    let revision = handle.revision.load(Ordering::SeqCst);
    // 取消代数已递增
    assert_eq!(revision, 1);
    // 这时不能产出 ACK
    assert!(commit.finish(&mut protocol, revision, now).is_err());
    // 队列里确实有那条取消命令
    assert!(matches!(
        rx.try_recv().unwrap(),
        Event::Command(super::super::QueuedCommand {
            command: Command::Active { value: false },
            ..
        })
    ));
}

#[test]
// 队列已满不会让已撤销的保存资格复活
fn queue_full_does_not_restore_a_revoked_commit() {
    let now = Instant::now();
    let (mut protocol, _, commit) = pending(now);
    // 队列只有一格
    let (handle, _rx) = handle(1);
    // 先塞满
    handle.command(r#"{"op":"refresh"}"#).unwrap();
    // 取消命令因队列已满入队失败
    assert!(handle.command(r#"{"op":"active","value":false}"#).is_err());
    // 但取消代数已递增，保存资格仍保持失效
    assert!(
        commit
            .finish(&mut protocol, handle.revision.load(Ordering::SeqCst), now)
            .is_err()
    );
}

#[test]
// 忘记另一台桌面不会取消当前的配对
fn forgetting_another_host_does_not_cancel_the_current_pairing() {
    let now = Instant::now();
    let (mut protocol, sender, commit) = pending(now);
    let (handle, _rx) = handle(1);
    // 忘记的是另一台桌面
    handle
        .command(&serde_json::json!({"op":"forget","clientId":OTHER}).to_string())
        .unwrap();
    // 取消代数不变
    assert_eq!(handle.revision.load(Ordering::SeqCst), 0);
    // 当前配对仍能完成
    let ack = commit.finish(&mut protocol, 0, now).unwrap();
    assert_eq!(sender.verify(ack).unwrap().client_id(), CLIENT);
}

#[test]
// 恰在窗口截止时刻才写完存储，不能产出 ACK
fn a_store_that_finishes_at_the_window_deadline_cannot_produce_ack() {
    let now = Instant::now();
    let (mut protocol, _, commit) = pending(now);
    // 窗口截止前 1 秒检查仍有效
    commit
        .check(&mut protocol, 0, now + Duration::from_secs(59))
        .unwrap();
    // 真实记录可能已保存；截止后只拒绝 ACK，不声称撤销磁盘提交。
    // 到截止时刻才完成保存：真实记录可能已写入，只拒绝 ACK，不声称撤销磁盘提交
    assert!(
        commit
            .finish(&mut protocol, 0, now + Duration::from_secs(60))
            .is_err()
    );
}

#[test]
// 连接失败不能重置五次尝试的预算
fn failed_connections_cannot_reset_the_five_attempt_budget() {
    let now = Instant::now();
    let mut protocol = Protocol::default();
    protocol
        .start_pairing(CLIENT, "本轮名称", "0037".into(), 0, now)
        .unwrap();
    // 用错误的码连续尝试 5 次，每次都失败并断开
    for _ in 0..5 {
        let (sender, hello) = SenderSession::start(CLIENT, DEVICE, "7777", SESSION.into()).unwrap();
        let reply = protocol.hello(hello, DEVICE, now).unwrap();
        let (_, confirmation) = sender.accept(reply).unwrap();
        assert!(protocol.confirmation(confirmation, now).is_err());
        // 断开连接不会重置尝试预算
        protocol.disconnect();
    }
    // 第 6 次即使码正确也被拒绝
    let (_, hello) = SenderSession::start(CLIENT, DEVICE, "0037", SESSION.into()).unwrap();
    assert!(protocol.hello(hello, DEVICE, now).is_err());
}

#[test]
// 过期的保存资格不会消耗新的配对窗口
fn stale_commit_does_not_consume_a_new_window() {
    let now = Instant::now();
    // 先有一个旧的保存资格
    let (mut protocol, _, old) = pending(now);
    // 然后开启新的配对窗口
    protocol
        .start_pairing(CLIENT, "下一轮", "1122".into(), 1, now)
        .unwrap();
    let (sender, hello) = SenderSession::start(CLIENT, DEVICE, "1122", SESSION.into()).unwrap();
    let reply = protocol.hello(hello, DEVICE, now).unwrap();
    // 旧资格完成失败，且不消耗新窗口
    assert!(old.finish(&mut protocol, 1, now).is_err());
    // 新窗口的配对照常完成
    let (sender, confirmation) = sender.accept(reply).unwrap();
    let commit = protocol.confirmation(confirmation, now).unwrap();
    let ack = commit.finish(&mut protocol, 1, now).unwrap();
    assert_eq!(sender.verify(ack).unwrap().device_name(), "下一轮");
}
