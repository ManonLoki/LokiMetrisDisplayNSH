// 引入被测命令相关类型
use super::super::{
    Endpoint, Event,
    connection_test::{TestFiles, files, owner_for_test},
};
use super::*;
use std::sync::mpsc;

// 两台测试桌面的 ID
const A: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const B: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

// 取出下一条已入队的命令
fn next(rx: &mpsc::Receiver<Event>) -> QueuedCommand {
    match rx.try_recv().unwrap() {
        Event::Command(command) => command,
        _ => panic!("expected a queued command"),
    }
}

// 构造句柄、事件接收端与 owner
fn fixture(capacity: usize) -> (Handle, mpsc::Receiver<Event>, Owner, TestFiles) {
    let files = files();
    // 句柄与事件接收端，初始意图选中 A
    let (handle, rx) = Handle::for_test(capacity, Some(A));
    let (mut owner, _tx, _owner_rx) = owner_for_test(&files);
    // owner 与句柄共用取消代数
    owner.revision = handle.revision.clone();
    // 当前选中 A
    owner.selected = Some(A.into());
    owner.view["selectedId"] = json!(A);
    // 后台 owner 可验证完整命令顺序，同时不会向无关进程发起 TCP。
    // 后台 owner 可以验证完整命令顺序，又不会向无关进程发起 TCP
    owner.lifecycle.permitted = false;
    // 两台桌面都已发现
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
    // 为 A 开启一个进行中的配对
    owner
        .protocol
        .start_pairing(A, &owner.name, "0037".into(), 0, Instant::now())
        .unwrap();
    owner.publish();
    (handle, rx, owner, files)
}

#[test]
#[cfg(target_os = "macos")]
// 选择 B 再忘记 A：即使公开视图仍显示 A，也保留 B 的选择
fn select_b_then_forget_a_keeps_b_while_the_published_view_still_shows_a() {
    let (handle, rx, mut owner, _files) = fixture(2);
    // 先选择 B，再忘记 A，两条命令都入队成功
    handle
        .enqueue(Command::Select {
            client_id: B.into(),
        })
        .unwrap();
    handle
        .enqueue(Command::Forget {
            client_id: A.into(),
        })
        // 入队成功
        .unwrap();
    // 模拟 Keychain 保存阻塞：两条命令已成功入队，投影还没有前进。
    // 模拟钥匙串保存阻塞：命令已入队，但公开视图尚未前进
    assert_eq!(owner.published_view()["selectedId"], A);
    // 只有“选择”中断了配对，取消代数为 1
    assert_eq!(handle.revision.load(Ordering::SeqCst), 1);
    // owner 依序处理两条命令
    owner.apply(next(&rx)).unwrap();
    owner.apply(next(&rx)).unwrap();
    // 最终仍选中 B
    assert_eq!(owner.selected.as_deref(), Some(B));
    // B 的配对窗口已开启
    assert_eq!(owner.protocol.pairing_revision(), Some(1));
    assert!(owner.pairing_offer.is_some());
}

#[test]
#[cfg(target_os = "macos")]
// 选择 B 再忘记 B：不会让 A 被选中
fn select_b_then_forget_b_does_not_leave_a_selected() {
    let (handle, rx, mut owner, _files) = fixture(2);
    handle
        .enqueue(Command::Select {
            client_id: B.into(),
        })
        .unwrap();
    handle
        // 入队忘记命令
        .enqueue(Command::Forget {
            // 目标客户端
            client_id: B.into(),
        })
        // 入队成功
        .unwrap();
    // 先选 B 再忘记 B：两条都中断配对，代数为 2
    assert_eq!(handle.revision.load(Ordering::SeqCst), 2);
    owner.apply(next(&rx)).unwrap();
    // 处理完选择：选中 B，但旧代数的配对不会开启
    assert_eq!(owner.selected.as_deref(), Some(B));
    assert!(owner.protocol.pairing_revision().is_none());
    owner.apply(next(&rx)).unwrap();
    // 处理完忘记：没有选中，也没有配对邀请
    assert!(owner.selected.is_none());
    assert!(owner.pairing_offer.is_none());
}

#[test]
// 先进入后台再选择：保留选择但不发起连接
fn background_then_select_preserves_the_selection_without_connecting() {
    let (handle, rx, mut owner, _files) = fixture(2);
    // 先让 owner 处于允许连接的状态
    owner.lifecycle.permitted = true;
    // 进入后台后再选择 B
    handle.enqueue(Command::Active { value: false }).unwrap();
    handle
        .enqueue(Command::Select {
            client_id: B.into(),
        })
        .unwrap();
    owner.apply(next(&rx)).unwrap();
    owner.apply(next(&rx)).unwrap();
    // 选中项应为 B
    assert_eq!(owner.selected.as_deref(), Some(B));
    // owner 不活跃、没有套接字
    assert!(!owner.active());
    assert!(owner.socket.is_none());
    // 后台不开启配对窗口。
    // 后台不开启配对窗口
    assert!(owner.protocol.pairing_revision().is_none());
}

#[test]
// 先选择再进入后台：保留选择，但绝不打开已被撤销的配对窗口
fn select_then_background_keeps_the_selection_but_never_opens_its_revoked_window() {
    let (handle, rx, mut owner, _files) = fixture(2);
    handle
        .enqueue(Command::Select {
            client_id: B.into(),
        })
        .unwrap();
    // 先选择，再进入后台
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
// 已接受的停止命令是终态，即使 owner 尚未消费
fn accepted_stop_is_terminal_even_if_the_owner_has_not_consumed_it() {
    let (handle, rx, mut owner, _files) = fixture(2);
    // 停止命令入队后即为终态
    handle.enqueue(Command::Stop).unwrap();
    // 之后任何命令都被拒绝
    assert!(
        handle
            .enqueue(Command::Select {
                client_id: B.into()
            })
            .is_err()
    );
    // owner 运行时遇到停止就退出
    owner.run(rx);
    // 排在停止之后的选择没有被执行
    assert_eq!(owner.selected.as_deref(), Some(A));
    assert!(owner.protocol.pairing_revision().is_none());
}

#[test]
// 被拒绝的选择不会替换最近一次成功入队的意图
fn rejected_selection_does_not_replace_the_last_successfully_queued_intent() {
    // 队列只有一格
    let (handle, _rx, _owner, _files) = fixture(1);
    // 先塞满
    handle.enqueue(Command::Refresh).unwrap();
    // 被拒绝的选择和忘记不能改变意图
    assert!(
        handle
            .enqueue(Command::Select {
                client_id: B.into()
            })
            .is_err()
    );
    assert!(
        // 句柄
        handle
            // 入队忘记命令
            .enqueue(Command::Forget {
                // 目标客户端
                client_id: A.into()
            })
            // 应被拒绝
            .is_err()
    );
    // 但取消代数仍已递增（保存资格保持失效）
    assert_eq!(handle.revision.load(Ordering::SeqCst), 2);
    // 意图仍是最后一次成功入队的 A
    assert_eq!(handle.intent.lock().unwrap().selected.as_deref(), Some(A));
}

#[test]
// 命令队列已满时，也不能让 owner 在 VM 关闭后继续存活
fn a_full_command_queue_cannot_keep_the_owner_alive_after_vm_close() {
    let files = super::super::connection_test::files();
    let (handle, receiver) = super::super::Handle::for_test(1, None);
    let mut owner = super::super::owner::Owner::for_test(
        files.root.clone(),
        &files.device,
        "NSH",
        &files.service,
        // 克隆发送端
        handle.tx.clone(),
    );
    // owner 共享句柄的停止标记
    owner.stopped = handle.stopped.clone();
    // 队列塞满后关闭 VM（丢弃句柄）：停止命令无法入队
    handle.command("{\"op\":\"discover\"}").unwrap();
    drop(handle);
    let (done, completed) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        owner.run(receiver);
        done.send(()).unwrap();
    });
    // 即使停止命令入不了队，owner 也必须在 2 秒内退出
    completed
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("closing an NSH VM must terminate its owner even when the stop queue was full");
    worker.join().unwrap();
}
