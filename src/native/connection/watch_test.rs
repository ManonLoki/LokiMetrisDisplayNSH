//! 生产WC终态控制器→有界owner事件→真实加密TCP的回归，夹具只用隔离身份和本机回环。
use super::{
    Event, HANDSHAKE_FRAME, MAX_FRAME, Material,
    connection_test::{
        CLIENT, command, configured, files, owner_for_test, server_recv, server_send, snapshot,
    },
};
use crate::native::companion::presence::{
    Command as PresenceCommand, Controller, ExpectedSnapshot, ProbeRequest, WireReport,
};
use loki_metris_pairing::session::AwaitingDeviceProof;
use serde_json::{Value, json};
use std::{
    io::Read,
    net::{TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

const WATCH: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const STREAM: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const FOREGROUND: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";

/// 使用同一生产恢复握手：终端第一条明文仍是原有裸DeviceInfo。
fn resume(
    listener: &TcpListener,
    device: &str,
    capabilities: Value,
) -> (TcpStream, loki_metris_pairing::session::SecureChannel) {
    let mut socket = configured(listener.accept().unwrap().0);
    let intro = server_recv(&mut socket, HANDSHAKE_FRAME);
    assert_eq!(intro["kind"], "resume");
    let (waiting, reply) = AwaitingDeviceProof::accept(
        serde_json::from_value(intro["value"].clone()).unwrap(),
        CLIENT,
        device,
        &[7; 32],
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
    assert!(info.get("type").is_none());
    let packet = channel.seal(&serde_json::to_vec(&json!({"type":"snapshot","snapshot":snapshot(vec![]),"capabilities":capabilities,"widgetEndpoint":null})).unwrap()).unwrap();
    server_send(&mut socket, &serde_json::to_value(packet).unwrap());
    (socket, channel)
}
/// 所有应用消息均走生产SecureChannel，不允许明文旁路。
fn receive(
    socket: &mut TcpStream,
    channel: &mut loki_metris_pairing::session::SecureChannel,
) -> Value {
    serde_json::from_slice(
        &channel
            .open(&serde_json::from_value(server_recv(socket, MAX_FRAME)).unwrap())
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn true_watch_callback_uses_current_authenticated_capability_generation_and_zero_nonce() {
    let files = files();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (mut owner, tx, rx) = owner_for_test(&files);
    owner.records.insert(
        CLIENT.into(),
        Material {
            client_id: CLIENT.into(),
            device_id: files.device.clone(),
            name: "隔离桌面".into(),
            secret: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [7; 32]),
            saved_at_ms: 1,
        },
    );
    owner.selected = Some(CLIENT.into());
    owner.endpoints.insert(
        "fixture".into(),
        super::Endpoint {
            id: CLIENT.into(),
            name: "隔离桌面".into(),
            address: listener.local_addr().unwrap(),
        },
    );
    // connect在生产路径中先重置回调scope，再创建当前socket代数。
    owner.connect().unwrap();
    let generation = owner.generation;
    let scope = owner.watch.scope.clone();
    let worker = thread::spawn(move || owner.run(rx));
    let (mut socket, mut channel) = resume(&listener, &files.device, json!(["watchCompanionV1"]));
    let ping = channel
        .seal(&serde_json::to_vec(&json!({"type":"ping","nonce":0})).unwrap())
        .unwrap();
    server_send(&mut socket, &serde_json::to_value(ping).unwrap());
    assert_eq!(
        receive(&mut socket, &mut channel),
        json!({"type":"pong","nonce":0})
    );
    let report = WireReport {
        info: crate::native::companion::presence::WatchInfo {
            device_id: WATCH.into(),
            name: "不应转发的迟到回执".into(),
            app_version: "1.0.15".into(),
        },
        reachable: false,
        contact: None,
        sync: None,
    };
    // 三种旧资格不能提前出现在TCP；后续下一帧必须来自真终态入口。
    for (g, s, n) in [
        (generation - 1, scope.clone(), 0),
        (generation, "old-scope".into(), 0),
        (generation, scope.clone(), 9),
    ] {
        tx.send(Event::Companion {
            generation: g,
            scope: s,
            nonce: n,
            report: report.clone(),
        })
        .unwrap();
    }
    let sink_tx = tx.clone();
    let sink_scope = scope.clone();
    let mut controller = Controller::default();
    controller.command(PresenceCommand::Probe(ProbeRequest {
        root: files.root.join("companion-v1"),
        source_id: CLIENT.into(),
        scope: scope.clone(),
        sink: Arc::new(move |report| {
            sink_tx
                .send(Event::Companion {
                    generation,
                    scope: sink_scope.clone(),
                    nonce: 0,
                    report,
                })
                .unwrap();
        }),
    }));
    let now = Instant::now();
    let (challenge, _) = controller
        .start(
            ExpectedSnapshot {
                stream_id: STREAM.into(),
                revision: "1".into(),
                source_id: CLIENT.into(),
            },
            now,
        )
        .unwrap();
    let bytes = serde_json::to_vec(&json!({"version":1,"type":"presenceReply","challenge":challenge,
        "info":{"deviceId":WATCH,"name":"真实回执夹具Watch","appVersion":"1.0.15"},"active":true,"foregroundId":FOREGROUND,
        "sync":{"streamId":STREAM,"revision":"1","sourceId":CLIENT,"ageMs":0}})).unwrap();
    controller.complete(&challenge, Some(&bytes), now);
    let wire = receive(&mut socket, &mut channel);
    assert_eq!(wire["type"], "companionStatus");
    assert_eq!(wire["nonce"], 0);
    assert_eq!(wire["report"]["info"]["name"], "真实回执夹具Watch");
    assert_eq!(wire["report"]["reachable"], true);
    assert_eq!(wire["report"]["contact"]["sequence"], 1);
    assert_eq!(wire["report"]["sync"]["sequence"], 1);
    // 当前完整snapshot撤销capability，下一ping仍pong但回执不能再发。
    let packet = channel.seal(&serde_json::to_vec(&json!({"type":"snapshot","snapshot":snapshot(vec![]),"capabilities":[],"widgetEndpoint":null})).unwrap()).unwrap();
    server_send(&mut socket, &serde_json::to_value(packet).unwrap());
    let ping = channel
        .seal(&serde_json::to_vec(&json!({"type":"ping","nonce":1})).unwrap())
        .unwrap();
    server_send(&mut socket, &serde_json::to_value(ping).unwrap());
    assert_eq!(
        receive(&mut socket, &mut channel),
        json!({"type":"pong","nonce":1})
    );
    tx.send(Event::Companion {
        generation,
        scope: scope.clone(),
        nonce: 0,
        report,
    })
    .unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let error = socket.read(&mut [0u8; 1]).unwrap_err();
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ));
    tx.send(command(super::Command::Stop)).unwrap();
    worker.join().unwrap();
}
