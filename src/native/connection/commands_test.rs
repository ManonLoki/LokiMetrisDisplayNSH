use super::super::{
    Endpoint, Event,
    connection_test::{TestFiles, files, owner_for_test},
};
use super::*;
use std::sync::mpsc;

const A: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

fn next(rx: &mpsc::Receiver<Event>) -> QueuedCommand {
    match rx.try_recv().unwrap() {
        Event::Command(command) => command,
        _ => panic!("expected a queued command"),
    }
}

fn fixture(capacity: usize) -> (Handle, mpsc::Receiver<Event>, Owner, TestFiles) {
    let files = files();
    let (handle, rx) = Handle::for_test(capacity, Some(A));
    let (mut owner, _tx, _owner_rx) = owner_for_test(&files);
    owner.revision = handle.revision.clone();
    owner.selected = Some(A.into());
    owner.view["selectedId"] = json!(A);
    // 后台 owner 可验证完整命令顺序，同时不会向无关进程发起 TCP。
    owner.lifecycle.permitted = false;
    for id in [A, B] {
        owner.endpoints.insert(
            id.into(),
            Endpoint {
                id: id.into(),
                name: "顺序测试桌面".into(),
                address: "127.0.0.1:9".parse().unwrap(),
            },
        );
    }
    owner
        .protocol
        .start_pairing(A, &owner.name, "0037".into(), 0, Instant::now())
        .unwrap();
    owner.publish();
    (handle, rx, owner, files)
}

#[test]
#[cfg(target_os = "macos")]
fn select_b_then_forget_a_keeps_b_while_the_published_view_still_shows_a() {
    let (handle, rx, mut owner, _files) = fixture(2);
    handle
        .enqueue(Command::Select {
            client_id: B.into(),
        })
        .unwrap();
    handle
        .enqueue(Command::Forget {
            client_id: A.into(),
        })
        .unwrap();
    // 模拟 Keychain 保存阻塞：两条命令已成功入队，投影还没有前进。
    assert_eq!(owner.published_view()["selectedId"], A);
    assert_eq!(handle.revision.load(Ordering::SeqCst), 1);
    owner.apply(next(&rx)).unwrap();
    owner.apply(next(&rx)).unwrap();
    assert_eq!(owner.selected.as_deref(), Some(B));
    assert_eq!(owner.protocol.pairing_revision(), Some(1));
    assert!(owner.pairing_offer.is_some());
}

#[test]
#[cfg(target_os = "macos")]
fn select_b_then_forget_b_does_not_leave_a_selected() {
    let (handle, rx, mut owner, _files) = fixture(2);
    handle
        .enqueue(Command::Select {
            client_id: B.into(),
        })
        .unwrap();
    handle
        .enqueue(Command::Forget {
            client_id: B.into(),
        })
        .unwrap();
    assert_eq!(handle.revision.load(Ordering::SeqCst), 2);
    owner.apply(next(&rx)).unwrap();
    assert_eq!(owner.selected.as_deref(), Some(B));
    assert!(owner.protocol.pairing_revision().is_none());
    owner.apply(next(&rx)).unwrap();
    assert!(owner.selected.is_none());
    assert!(owner.pairing_offer.is_none());
}

#[test]
fn background_then_select_preserves_the_selection_without_connecting() {
    let (handle, rx, mut owner, _files) = fixture(2);
    owner.lifecycle.permitted = true;
    handle.enqueue(Command::Active { value: false }).unwrap();
    handle
        .enqueue(Command::Select {
            client_id: B.into(),
        })
        .unwrap();
    owner.apply(next(&rx)).unwrap();
    owner.apply(next(&rx)).unwrap();
    assert_eq!(owner.selected.as_deref(), Some(B));
    assert!(!owner.active());
    assert!(owner.socket.is_none());
    // 后台不开启配对窗口。
    assert!(owner.protocol.pairing_revision().is_none());
}

#[test]
fn select_then_background_keeps_the_selection_but_never_opens_its_revoked_window() {
    let (handle, rx, mut owner, _files) = fixture(2);
    handle
        .enqueue(Command::Select {
            client_id: B.into(),
        })
        .unwrap();
    handle.enqueue(Command::Active { value: false }).unwrap();
    owner.apply(next(&rx)).unwrap();
    assert_eq!(owner.selected.as_deref(), Some(B));
    assert!(owner.protocol.pairing_revision().is_none());
    assert!(owner.socket.is_none());
    owner.apply(next(&rx)).unwrap();
    assert!(!owner.active());
    assert!(owner.pairing_offer.is_none());
}

#[test]
fn accepted_stop_is_terminal_even_if_the_owner_has_not_consumed_it() {
    let (handle, rx, mut owner, _files) = fixture(2);
    handle.enqueue(Command::Stop).unwrap();
    assert!(
        handle
            .enqueue(Command::Select {
                client_id: B.into()
            })
            .is_err()
    );
    owner.run(rx);
    assert_eq!(owner.selected.as_deref(), Some(A));
    assert!(owner.protocol.pairing_revision().is_none());
}

#[test]
fn rejected_selection_does_not_replace_the_last_successfully_queued_intent() {
    let (handle, _rx, _owner, _files) = fixture(1);
    handle.enqueue(Command::Refresh).unwrap();
    assert!(
        handle
            .enqueue(Command::Select {
                client_id: B.into()
            })
            .is_err()
    );
    assert!(
        handle
            .enqueue(Command::Forget {
                client_id: A.into()
            })
            .is_err()
    );
    assert_eq!(handle.revision.load(Ordering::SeqCst), 2);
    assert_eq!(handle.intent.lock().unwrap().selected.as_deref(), Some(A));
}

#[test]
fn a_full_command_queue_cannot_keep_the_owner_alive_after_vm_close() {
    let files = super::super::connection_test::files();
    let (handle, receiver) = super::super::Handle::for_test(1, None);
    let mut owner = super::super::owner::Owner::for_test(
        files.root.clone(),
        &files.device,
        "NSH",
        &files.service,
        handle.tx.clone(),
    );
    owner.stopped = handle.stopped.clone();
    handle.command("{\"op\":\"discover\"}").unwrap();
    drop(handle);
    let (done, completed) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        owner.run(receiver);
        done.send(()).unwrap();
    });
    completed
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("closing an NSH VM must terminate its owner even when the stop queue was full");
    worker.join().unwrap();
}
