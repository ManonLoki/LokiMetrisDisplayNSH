use super::*;
use super::{
    connection_test::{amount, files, snapshot},
    framed_socket::{FramedSocket, SocketEvent},
    persistence::{atomic_write, file_url, private_dir},
    snapshot_admission::validate_snapshot,
};
use serde_json::{Value, json};
use std::{fs, io::Write, net::TcpListener, path::Path, time::Duration};

#[test]
// 原生快照边界在报告在线之前就拒绝格式错误的快照
fn native_snapshot_boundary_rejects_malformed_before_online() {
    let v = snapshot(vec![]);
    assert!(validate_snapshot(&v).is_ok());
    // 只有错误而没有统计也合法
    let mut unavailable = v.clone();
    unavailable["tokens"] = Value::Null;
    unavailable["tokenError"] = json!("unavailable");
    assert!(validate_snapshot(&unavailable).is_ok());
    // 非法总量：数字类型、前导零、超出 u64、负数
    for total in [
        json!(123),
        json!("01"),
        json!("18446744073709551616"),
        json!("-1"),
    ] {
        let mut bad = v.clone();
        bad["tokens"]["all"]["total"] = total;
        assert!(validate_snapshot(&bad).is_err());
    }
    let mut bad = v.clone();
    bad["tokens"]["agents"] =
        json!([{"agent":"codex","amount":amount("1")},{"agent":"codex","amount":amount("2")}]);
    assert!(validate_snapshot(&bad).is_err());
    // 窗口种类与字段不符的额度
    let mut bad = v.clone();
    bad["providerQuota"]["providers"] =
        json!([{"provider":"cursor","plan":"Pro","monthly":{"kind":"weekly","usedTenths":1}}]);
    assert!(validate_snapshot(&bad).is_err());
    // 槽位 13 超出范围的 Hook
    let mut bad = v;
    bad["hooks"] = json!([{"agent":"codex","slot":13,"behavior":"running","content":"","contentTruncated":false,"image":null}]);
    assert!(validate_snapshot(&bad).is_err());
}

#[test]
// TCP 帧长度上限在为远端数据分配内存之前检查
fn tcp_frame_limit_is_checked_before_allocating_remote_payload() {
    // 本机回环上的一条连接
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (tx, rx) = mpsc::sync_channel(8);
    let _wire = FramedSocket::connect(listener.local_addr().unwrap(), true, 1, tx);
    let (mut peer, _) = listener.accept().unwrap();
    // 只发一个超出握手上限的长度前缀，不发正文
    peer.write_all(&((HANDSHAKE_FRAME + 1) as u32).to_be_bytes())
        .unwrap();
    // 连接应立即以“无效网络帧长度”关闭
    match rx.recv_timeout(Duration::from_secs(2)).unwrap() {
        Event::Socket(1, SocketEvent::Closed(reason)) => assert_eq!(reason, "无效网络帧长度"),
        _ => panic!("expected the oversized handshake frame to close the wire"),
    }
}

#[test]
// 握手对端在 owner 推进阶段之前不能发送下一阶段的数据
fn handshake_peer_cannot_send_the_next_phase_before_the_owner_advances() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (tx, rx) = mpsc::sync_channel(8);
    let _wire = FramedSocket::connect(listener.local_addr().unwrap(), true, 7, tx);
    let (mut peer, _) = listener.accept().unwrap();
    // 一次性发出两帧：第二帧属于尚未到来的阶段
    let mut frames = Vec::new();
    for value in [json!({"first":1}), json!({"second":2})] {
        let bytes = serde_json::to_vec(&value).unwrap();
        frames.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        frames.extend_from_slice(&bytes);
    }
    peer.write_all(&frames).unwrap();
    // 连接应以“握手阶段提前发送数据”关闭
    match rx.recv_timeout(Duration::from_secs(2)).unwrap() {
        Event::Socket(7, SocketEvent::Closed(reason)) => assert_eq!(reason, "握手阶段提前发送数据"),
        _ => panic!("expected pre-sent handshake data to close the wire"),
    }
}

#[test]
// file URL 对私有路径做编码，且不接受远端给的文件名
fn file_url_encodes_private_paths_without_accepting_remote_filenames() {
    // 路径中的空格与中文被百分号编码
    assert_eq!(
        file_url(Path::new("/tmp/a b/中文.png")),
        "file:///tmp/a%20b/%E4%B8%AD%E6%96%87.png"
    );
    // 原子写入：连续写两次，内容为最后一次，且不留临时文件
    let files = files();
    private_dir(&files.root).unwrap();
    let path = files.root.join("content");
    atomic_write(&path, b"first").unwrap();
    atomic_write(&path, b"second").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"second");
    assert!(!path.with_extension("pending").exists());
}

#[test]
// 原生快照与投影后的快照共用同一套代理身份规则
fn native_and_projected_snapshot_share_agent_identity_rules() {
    let v = snapshot(vec![]);
    // NFC 相等的名称对 UI 而言是同一个智能体,因此网络边界也必须拒绝它们。
    // 经 NFC 归一化后相同的名称对界面是同一个代理，所以网络边界也必须拒绝
    let mut bad = v.clone();
    bad["tokens"]["agents"] = json!([
        {"agent":"caf\u{e9}","amount":amount("1")},
        {"agent":"cafe\u{301}","amount":amount("2")}
    ]);
    assert!(validate_snapshot(&bad).is_err());
    assert!(crate::features::connection::snapshot::project_snapshot(&bad, &Value::Null).is_none());
    // 启用列表里归一化后重复的名称
    let mut bad = v;
    bad["enabledAgents"] = json!(["claudeCode", "claude-code"]);
    assert!(validate_snapshot(&bad).is_err());
}
