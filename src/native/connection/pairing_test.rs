use super::super::{Command, Event, Handle};
use super::*;
use loki_metris_pairing::{AwaitingAck, SenderSession};
use std::{
    sync::{atomic::Ordering, mpsc},
    time::Duration,
};

const CLIENT: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const OTHER: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const DEVICE: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const SESSION: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";

fn pending(now: Instant) -> (Protocol, AwaitingAck, Commit) {
    let mut protocol = Protocol::default();
    protocol
        .start_pairing(CLIENT, "本轮名称", "0037".into(), 0, now)
        .unwrap();
    let (sender, hello) = SenderSession::start(CLIENT, DEVICE, "0037", SESSION.into()).unwrap();
    let reply = protocol.hello(hello, DEVICE, now).unwrap();
    let (sender, confirmation) = sender.accept(reply).unwrap();
    let commit = protocol.confirmation(confirmation, now).unwrap();
    (protocol, sender, commit)
}

fn handle(capacity: usize) -> (Handle, mpsc::Receiver<Event>) {
    Handle::for_test(capacity, Some(CLIENT))
}

#[test]
fn pairing_transcript_keeps_the_name_that_opened_the_window() {
    let now = Instant::now();
    let mut protocol = Protocol::default();
    let mut displayed_name = "本轮名称".to_owned();
    protocol
        .start_pairing(CLIENT, &displayed_name, "0037".into(), 0, now)
        .unwrap();
    displayed_name = "随后编辑的新名称".into();
    let (sender, hello) = SenderSession::start(CLIENT, DEVICE, "0037", SESSION.into()).unwrap();
    let reply = protocol.hello(hello, DEVICE, now).unwrap();
    assert_eq!(reply.name, "本轮名称");
    assert_ne!(reply.name, displayed_name);
    let (sender, confirmation) = sender.accept(reply).unwrap();
    let commit = protocol.confirmation(confirmation, now).unwrap();
    assert_eq!(commit.material().device_name(), "本轮名称");
    let ack = commit.finish(&mut protocol, 0, now).unwrap();
    assert_eq!(sender.verify(ack).unwrap().device_name(), "本轮名称");
}

#[test]
fn queued_cancel_revokes_ack_before_the_owner_can_drain_its_commands() {
    let now = Instant::now();
    let (mut protocol, _, commit) = pending(now);
    commit.check(&mut protocol, 0, now).unwrap();
    let (handle, rx) = handle(1);
    // 模拟 OS 保存阻塞期间 UI 提交取消；队列仍未由 owner 消费。
    handle.command(r#"{"op":"active","value":false}"#).unwrap();
    let revision = handle.revision.load(Ordering::SeqCst);
    assert_eq!(revision, 1);
    assert!(commit.finish(&mut protocol, revision, now).is_err());
    assert!(matches!(
        rx.try_recv().unwrap(),
        Event::Command(super::super::QueuedCommand {
            command: Command::Active { value: false },
            ..
        })
    ));
}

#[test]
fn queue_full_does_not_restore_a_revoked_commit() {
    let now = Instant::now();
    let (mut protocol, _, commit) = pending(now);
    let (handle, _rx) = handle(1);
    handle.command(r#"{"op":"refresh"}"#).unwrap();
    assert!(handle.command(r#"{"op":"active","value":false}"#).is_err());
    assert!(
        commit
            .finish(&mut protocol, handle.revision.load(Ordering::SeqCst), now)
            .is_err()
    );
}

#[test]
fn forgetting_another_host_does_not_cancel_the_current_pairing() {
    let now = Instant::now();
    let (mut protocol, sender, commit) = pending(now);
    let (handle, _rx) = handle(1);
    handle
        .command(&serde_json::json!({"op":"forget","clientId":OTHER}).to_string())
        .unwrap();
    assert_eq!(handle.revision.load(Ordering::SeqCst), 0);
    let ack = commit.finish(&mut protocol, 0, now).unwrap();
    assert_eq!(sender.verify(ack).unwrap().client_id(), CLIENT);
}

#[test]
fn a_store_that_finishes_at_the_window_deadline_cannot_produce_ack() {
    let now = Instant::now();
    let (mut protocol, _, commit) = pending(now);
    commit
        .check(&mut protocol, 0, now + Duration::from_secs(59))
        .unwrap();
    // 真实记录可能已保存；截止后只拒绝 ACK，不声称撤销磁盘提交。
    assert!(
        commit
            .finish(&mut protocol, 0, now + Duration::from_secs(60))
            .is_err()
    );
}

#[test]
fn failed_connections_cannot_reset_the_five_attempt_budget() {
    let now = Instant::now();
    let mut protocol = Protocol::default();
    protocol
        .start_pairing(CLIENT, "本轮名称", "0037".into(), 0, now)
        .unwrap();
    for _ in 0..5 {
        let (sender, hello) = SenderSession::start(CLIENT, DEVICE, "7777", SESSION.into()).unwrap();
        let reply = protocol.hello(hello, DEVICE, now).unwrap();
        let (_, confirmation) = sender.accept(reply).unwrap();
        assert!(protocol.confirmation(confirmation, now).is_err());
        protocol.disconnect();
    }
    let (_, hello) = SenderSession::start(CLIENT, DEVICE, "0037", SESSION.into()).unwrap();
    assert!(protocol.hello(hello, DEVICE, now).is_err());
}

#[test]
fn stale_commit_does_not_consume_a_new_window() {
    let now = Instant::now();
    let (mut protocol, _, old) = pending(now);
    protocol
        .start_pairing(CLIENT, "下一轮", "1122".into(), 1, now)
        .unwrap();
    let (sender, hello) = SenderSession::start(CLIENT, DEVICE, "1122", SESSION.into()).unwrap();
    let reply = protocol.hello(hello, DEVICE, now).unwrap();
    assert!(old.finish(&mut protocol, 1, now).is_err());
    let (sender, confirmation) = sender.accept(reply).unwrap();
    let commit = protocol.confirmation(confirmation, now).unwrap();
    let ack = commit.finish(&mut protocol, 1, now).unwrap();
    assert_eq!(sender.verify(ack).unwrap().device_name(), "下一轮");
}
