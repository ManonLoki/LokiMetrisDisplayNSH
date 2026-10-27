use super::*;
use super::{
    commands::QueuedCommand,
    framed_socket::{FramedSocket, SocketEvent},
    owner::Owner,
    persistence::{atomic_write, file_url, private_dir},
    snapshot_admission::validate_snapshot,
};
use loki_metris_pairing::{SenderSession, assets::AssetDescriptor, session::AwaitingDeviceProof};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::mpsc::Receiver,
    thread,
    time::{Duration, Instant},
};

pub(super) const CLIENT: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
pub(super) fn amount(total: &str) -> Value {
    json!({"total":total,"availability":"available","completeness":"complete","confidence":"exact"})
}
pub(super) fn snapshot(assets: Vec<AssetDescriptor>) -> Value {
    json!({"version":1,"tokens":{"day":{"date":"2026-10-26","zone":"UTC","startMs":1,"endMs":100},"observedMs":50,"all":amount("18446744073709551615"),"agents":[{"agent":"codex","amount":amount("0")}]},"tokenError":null,"hooksAvailable":true,"hooks":assets.first().map(|d|vec![json!({"agent":"codex","slot":1,"behavior":"running","content":"真实 TCP 中文📱","contentTruncated":false,"image":d.hash})]).unwrap_or_default(),"assets":assets,"catalogError":null,"enabledAgents":[],"providerQuota":{"observedAtUnix":1,"providers":[]}})
}
pub(super) fn server_send(s: &mut TcpStream, v: &Value) {
    let b = serde_json::to_vec(v).unwrap();
    s.write_all(&(b.len() as u32).to_be_bytes()).unwrap();
    s.write_all(&b).unwrap();
}
pub(super) fn server_recv(s: &mut TcpStream, max: usize) -> Value {
    let mut size = [0; 4];
    s.read_exact(&mut size).unwrap();
    let size = u32::from_be_bytes(size) as usize;
    assert!(size > 0 && size <= max);
    let mut b = vec![0; size];
    s.read_exact(&mut b).unwrap();
    serde_json::from_slice(&b).unwrap()
}
pub(super) fn configured(s: TcpStream) -> TcpStream {
    s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    s.set_write_timeout(Some(Duration::from_secs(15))).unwrap();
    s
}
fn view(published: &Published) -> Value {
    serde_json::from_str(&published.state.lock().unwrap().view).unwrap_or(Value::Null)
}
pub(super) fn until(published: &Published, predicate: impl Fn(&Value) -> bool) -> Value {
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        let v = view(published);
        if predicate(&v) {
            return v;
        }
        assert!(
            Instant::now() < end,
            "timed out: status={} error={}",
            v["status"],
            v["error"]
        );
        thread::sleep(Duration::from_millis(10));
    }
}
pub(super) fn command(command: Command) -> Event {
    Event::Command(QueuedCommand {
        command,
        revision: 0,
    })
}
pub(super) struct TestFiles {
    pub(super) root: PathBuf,
    pub(super) service: String,
    pub(super) device: String,
}
impl Drop for TestFiles {
    fn drop(&mut self) {
        let _ = crate::native::credentials::delete(&self.service, &self.device);
        let _ = fs::remove_dir_all(&self.root);
    }
}
pub(super) fn owner_for_test(files: &TestFiles) -> (Owner, SyncSender<Event>, Receiver<Event>) {
    let (tx, rx) = mpsc::sync_channel(256);
    let owner = Owner::for_test(
        files.root.clone(),
        &files.device,
        "隔离测试终端📱",
        &files.service,
        tx.clone(),
    );
    (owner, tx, rx)
}
pub(super) fn files() -> TestFiles {
    let id = uuid::Uuid::new_v4().to_string();
    TestFiles {
        root: std::env::temp_dir().join(format!("loki-link-test-{id}")),
        service: format!("com.manonloki.loki-link-test.{id}"),
        device: id,
    }
}

/// Exercises the production owner and real loopback sockets, with an isolated real macOS Keychain entry.
/// The test reports the fixture's mobile platform; it does not assert legacy services accept desktop metadata.
#[test]
#[cfg(target_os = "macos")]
fn tcp_owner_pairs_commits_keychain_resumes_syncs_assets_and_rejects_replay() {
    let files = files();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (code_tx, code_rx) = mpsc::sync_channel::<String>(1);
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let device = files.device.clone();
    let mut bytes = b"GIF89a".to_vec();
    bytes.resize(36_111, 0x71);
    let descriptor = AssetDescriptor::from_bytes(&bytes).unwrap();
    let expected = bytes.clone();
    let hash = descriptor.hash.clone();
    let server = thread::spawn(move || {
        let mut socket = configured(listener.accept().unwrap().0);
        let intro = server_recv(&mut socket, HANDSHAKE_FRAME);
        assert_eq!(intro["kind"], "pairing");
        assert_eq!(intro["value"]["deviceId"], device);
        let code = code_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let (sender, hello) =
            SenderSession::start(CLIENT, &device, &code, uuid::Uuid::new_v4().to_string()).unwrap();
        server_send(&mut socket, &serde_json::to_value(hello).unwrap());
        let reply: loki_metris_pairing::PairingReply =
            serde_json::from_value(server_recv(&mut socket, HANDSHAKE_FRAME)).unwrap();
        assert_eq!(reply.name, "隔离测试终端📱");
        let (sender, confirmation) = sender.accept(reply).unwrap();
        server_send(&mut socket, &serde_json::to_value(confirmation).unwrap());
        let material = sender
            .verify(serde_json::from_value(server_recv(&mut socket, HANDSHAKE_FRAME)).unwrap())
            .unwrap();
        drop(socket);
        let mut socket = configured(listener.accept().unwrap().0);
        let intro = server_recv(&mut socket, HANDSHAKE_FRAME);
        assert_eq!(intro["kind"], "resume");
        let (waiting, reply) = AwaitingDeviceProof::accept(
            serde_json::from_value(intro["value"].clone()).unwrap(),
            CLIENT,
            &device,
            material.secret_bytes(),
            uuid::Uuid::new_v4().to_string(),
        )
        .unwrap();
        server_send(&mut socket, &serde_json::to_value(reply).unwrap());
        let mut channel = waiting
            .verify(serde_json::from_value(server_recv(&mut socket, HANDSHAKE_FRAME)).unwrap())
            .unwrap();
        let info: Value = serde_json::from_slice(
            &channel
                .open(&serde_json::from_value(server_recv(&mut socket, MAX_FRAME)).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(info["deviceId"], device);
        assert_eq!(info["platform"], "ios");
        let packet=channel.seal(&serde_json::to_vec(&json!({"type":"snapshot","snapshot":snapshot(vec![descriptor.clone()]),"capabilities":["snapshotQueryV1"]})).unwrap()).unwrap();
        server_send(&mut socket, &serde_json::to_value(&packet).unwrap());
        let mut offset = 0;
        while offset < descriptor.length {
            let request: Value = serde_json::from_slice(
                &channel
                    .open(&serde_json::from_value(server_recv(&mut socket, MAX_FRAME)).unwrap())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(request["type"], "assetRequest");
            assert_eq!(request["hash"], descriptor.hash);
            assert_eq!(request["offset"], offset);
            let chunk = descriptor.chunk(&bytes, offset).unwrap();
            offset += (descriptor.length - offset).min(16_384);
            let packet = channel
                .seal(&serde_json::to_vec(&json!({"type":"assetChunk","chunk":chunk})).unwrap())
                .unwrap();
            server_send(&mut socket, &serde_json::to_value(packet).unwrap());
        }
        let ping = channel.seal(br#"{"type":"ping","nonce":7}"#).unwrap();
        server_send(&mut socket, &serde_json::to_value(ping).unwrap());
        let pong: Value = serde_json::from_slice(
            &channel
                .open(&serde_json::from_value(server_recv(&mut socket, MAX_FRAME)).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(pong, json!({"type":"pong","nonce":7}));
        done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        // Replay the original snapshot ciphertext after later packets: the shared channel must close permanently.
        server_send(&mut socket, &serde_json::to_value(packet).unwrap());
        thread::sleep(Duration::from_millis(300));
    });
    let (mut owner, tx, rx) = owner_for_test(&files);
    let published = owner.published.clone();
    owner.endpoints.insert(
        "fixture".into(),
        Endpoint {
            id: CLIENT.into(),
            name: "环回验证台".into(),
            address,
        },
    );
    owner.publish();
    let worker = thread::spawn(move || owner.run(rx));
    tx.send(command(Command::Select {
        client_id: CLIENT.into(),
    }))
    .unwrap();
    let v = until(&published, |v| v["pairingCode"].is_string());
    tx.send(command(Command::Rename {
        name: "新的名称只影响后续连接".into(),
    }))
    .unwrap();
    until(&published, |value| {
        value["deviceName"] == "新的名称只影响后续连接"
    });
    code_tx
        .send(v["pairingCode"].as_str().unwrap().into())
        .unwrap();
    let v = until(&published, |v| {
        v["connected"] == true && v["images"][&hash].is_string()
    });
    assert_eq!(
        v["snapshot"]["tokens"]["all"]["total"],
        "18446744073709551615"
    );
    assert_eq!(v["live"], true);
    assert!(v["paired"][CLIENT].is_object());
    assert!(v.to_string().find("secret").is_none());
    assert_eq!(
        fs::read(files.root.join("assets").join(&hash)).unwrap(),
        expected
    );
    let companion: Value =
        serde_json::from_slice(&fs::read(files.root.join("companion-v1/manifest.json")).unwrap())
            .unwrap();
    assert_eq!(companion["sourceId"], CLIENT);
    assert_eq!(companion["receivedAtMs"], v["receivedAtMs"]);
    assert_eq!(
        companion["snapshot"]["tokens"]["all"]["total"],
        "18446744073709551615"
    );
    assert!(
        companion["snapshot"]["hooks"][0]
            .get("imageSource")
            .is_none()
    );
    assert_eq!(
        fs::read(files.root.join("companion-v1/assets").join(&hash)).unwrap(),
        expected
    );
    let stored = crate::native::credentials::read(&files.service, &files.device)
        .unwrap()
        .unwrap();
    let records: BTreeMap<String, Material> = serde_json::from_str(&stored).unwrap();
    assert_eq!(records[CLIENT].device_id, files.device);
    // Canonical private cache preserves source timestamp while disconnected, and restores verified images.
    let (mut cached, _cached_tx, _cached_rx) = owner_for_test(&files);
    cached.records = records;
    cached.restore_cache();
    assert!(cached.view["images"][&hash].is_string());
    assert_eq!(cached.view["live"], false);
    assert_eq!(cached.view["receivedAtMs"], v["receivedAtMs"]);
    // Forgetting the cached source clears all content even on desktop.
    cached.selected = Some(CLIENT.into());
    cached
        .command(Command::Forget {
            client_id: CLIENT.into(),
        })
        .unwrap();
    assert!(cached.view["snapshot"].is_null());
    assert_eq!(cached.view["images"], json!({}));
    done_tx.send(()).unwrap();
    until(&published, |v| {
        v["connected"] == false && !v["error"].is_null()
    });
    tx.send(command(Command::Stop)).unwrap();
    worker.join().unwrap();
    server.join().unwrap();
}

#[test]
fn native_snapshot_boundary_rejects_malformed_before_online() {
    let v = snapshot(vec![]);
    assert!(validate_snapshot(&v).is_ok());
    let mut unavailable = v.clone();
    unavailable["tokens"] = Value::Null;
    unavailable["tokenError"] = json!("unavailable");
    assert!(validate_snapshot(&unavailable).is_ok());
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
    let mut bad = v.clone();
    bad["providerQuota"]["providers"] =
        json!([{"provider":"cursor","plan":"Pro","monthly":{"kind":"weekly","usedTenths":1}}]);
    assert!(validate_snapshot(&bad).is_err());
    let mut bad = v;
    bad["hooks"] = json!([{"agent":"codex","slot":13,"behavior":"running","content":"","contentTruncated":false,"image":null}]);
    assert!(validate_snapshot(&bad).is_err());
}

#[test]
fn tcp_frame_limit_is_checked_before_allocating_remote_payload() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (tx, rx) = mpsc::sync_channel(8);
    let _wire = FramedSocket::connect(listener.local_addr().unwrap(), true, 1, tx);
    let (mut peer, _) = listener.accept().unwrap();
    peer.write_all(&((HANDSHAKE_FRAME + 1) as u32).to_be_bytes())
        .unwrap();
    match rx.recv_timeout(Duration::from_secs(2)).unwrap() {
        Event::Socket(1, SocketEvent::Closed(reason)) => assert_eq!(reason, "无效网络帧长度"),
        _ => panic!("expected the oversized handshake frame to close the wire"),
    }
}

#[test]
fn handshake_peer_cannot_send_the_next_phase_before_the_owner_advances() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (tx, rx) = mpsc::sync_channel(8);
    let _wire = FramedSocket::connect(listener.local_addr().unwrap(), true, 7, tx);
    let (mut peer, _) = listener.accept().unwrap();
    let mut frames = Vec::new();
    for value in [json!({"first":1}), json!({"second":2})] {
        let bytes = serde_json::to_vec(&value).unwrap();
        frames.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        frames.extend_from_slice(&bytes);
    }
    peer.write_all(&frames).unwrap();
    match rx.recv_timeout(Duration::from_secs(2)).unwrap() {
        Event::Socket(7, SocketEvent::Closed(reason)) => assert_eq!(reason, "握手阶段提前发送数据"),
        _ => panic!("expected pre-sent handshake data to close the wire"),
    }
}

#[test]
fn file_url_encodes_private_paths_without_accepting_remote_filenames() {
    assert_eq!(
        file_url(Path::new("/tmp/a b/中文.png")),
        "file:///tmp/a%20b/%E4%B8%AD%E6%96%87.png"
    );
    let files = files();
    private_dir(&files.root).unwrap();
    let path = files.root.join("content");
    atomic_write(&path, b"first").unwrap();
    atomic_write(&path, b"second").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"second");
    assert!(!path.with_extension("pending").exists());
}

#[test]
fn native_and_projected_snapshot_share_agent_identity_rules() {
    let v = snapshot(vec![]);
    // NFC-equal names are one agent for the UI, so the network boundary must reject them too.
    let mut bad = v.clone();
    bad["tokens"]["agents"] = json!([
        {"agent":"caf\u{e9}","amount":amount("1")},
        {"agent":"cafe\u{301}","amount":amount("2")}
    ]);
    assert!(validate_snapshot(&bad).is_err());
    assert!(crate::features::connection::snapshot::project_snapshot(&bad, &Value::Null).is_none());
    let mut bad = v;
    bad["enabledAgents"] = json!(["claudeCode", "claude-code"]);
    assert!(validate_snapshot(&bad).is_err());
}
