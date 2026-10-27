use super::super::{
    Command, Endpoint, Event, HANDSHAKE_FRAME, HostCommand, MAX_FRAME, Material,
    connection_test::{
        CLIENT, TestFiles, command, configured, files, owner_for_test, server_recv, server_send,
        snapshot, until,
    },
    owner::Owner,
    persistence,
};
use loki_metris_pairing::session::AwaitingDeviceProof;
use serde_json::{Value, json};
use std::{
    io::Read,
    net::{TcpListener, TcpStream},
    sync::mpsc::{Receiver, SyncSender},
    thread,
};

/// 已配对桌面：以保存的秘密恢复认证、交付一份快照，返回服务端加密通道。
fn resume_and_snapshot(
    listener: &TcpListener,
    device: &str,
    secret: &[u8; 32],
    capabilities: Value,
    endpoint: Value,
) -> (TcpStream, loki_metris_pairing::session::SecureChannel) {
    let mut socket = configured(listener.accept().unwrap().0);
    let intro = server_recv(&mut socket, HANDSHAKE_FRAME);
    assert_eq!(intro["kind"], "resume");
    let (waiting, reply) = AwaitingDeviceProof::accept(
        serde_json::from_value(intro["value"].clone()).unwrap(),
        CLIENT,
        device,
        secret,
        uuid::Uuid::new_v4().to_string(),
    )
    .unwrap();
    server_send(&mut socket, &serde_json::to_value(reply).unwrap());
    let mut channel = waiting
        .verify(serde_json::from_value(server_recv(&mut socket, HANDSHAKE_FRAME)).unwrap())
        .unwrap();
    let _info = channel
        .open(&serde_json::from_value(server_recv(&mut socket, MAX_FRAME)).unwrap())
        .unwrap();
    let packet = channel
        .seal(&serde_json::to_vec(&json!({"type":"snapshot","snapshot":snapshot(vec![]),"capabilities":capabilities,"widgetEndpoint":endpoint})).unwrap())
        .unwrap();
    server_send(&mut socket, &serde_json::to_value(packet).unwrap());
    (socket, channel)
}

fn trusted_owner(
    files: &TestFiles,
    address: std::net::SocketAddr,
) -> (Owner, SyncSender<Event>, Receiver<Event>, [u8; 32]) {
    let (mut owner, tx, rx) = owner_for_test(files);
    let secret = [7u8; 32];
    owner.records.insert(
        CLIENT.into(),
        Material {
            client_id: CLIENT.into(),
            device_id: files.device.clone(),
            name: "环回验证台".into(),
            secret: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, secret),
            saved_at_ms: 1,
        },
    );
    owner.selected = Some(CLIENT.into());
    owner.endpoints.insert(
        "fixture".into(),
        Endpoint {
            id: CLIENT.into(),
            name: "环回验证台".into(),
            address,
        },
    );
    (owner, tx, rx, secret)
}

#[test]
fn leaving_the_foreground_sends_pause_then_closes_and_reports_form_factor_once() {
    let files = files();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (mut owner, tx, rx, secret) = trusted_owner(&files, listener.local_addr().unwrap());
    owner.form_factor = Some("phone".into());
    let published = owner.published.clone();
    let worker = thread::spawn(move || owner.run(rx));
    tx.send(command(Command::Refresh)).unwrap();
    let device = files.device.clone();
    let (mut socket, mut channel) = resume_and_snapshot(
        &listener,
        &device,
        &secret,
        json!(["deviceFormFactorV1"]),
        Value::Null,
    );
    let mut next = |socket: &mut TcpStream| -> Value {
        serde_json::from_slice(
            &channel
                .open(&serde_json::from_value(server_recv(socket, MAX_FRAME)).unwrap())
                .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(
        next(&mut socket),
        json!({"type":"deviceFormFactor","formFactor":"phone"})
    );
    until(&published, |v| v["connected"] == true);
    tx.send(command(Command::Host(HostCommand::Lifecycle {
        visible: false,
        permitted: true,
    })))
    .unwrap();
    assert_eq!(next(&mut socket), json!({"type":"pause"}));
    let mut rest = Vec::new();
    socket.read_to_end(&mut rest).unwrap();
    assert!(
        rest.is_empty(),
        "the paused stream must close without further frames"
    );
    let v = until(&published, |v| v["connected"] == false);
    assert_eq!(v["status"], "已暂停，回到前台后恢复");
    tx.send(command(Command::Stop)).unwrap();
    worker.join().unwrap();
}

#[test]
#[cfg(target_os = "macos")]
fn a_retired_pairing_book_cannot_be_imported_or_block_a_new_book() {
    let files = files();
    let retired = "com.manonloki.lokimetris.display.pairing.v1";
    crate::native::credentials::write(retired, &files.device, "retired, invalid records").unwrap();
    let result = persistence::load_records(super::super::CREDENTIAL_SERVICE, &files.device);
    crate::native::credentials::delete(retired, &files.device).unwrap();
    assert!(
        result.unwrap().is_empty(),
        "a fresh NSH pairing book must not read retired records"
    );
}

#[test]
fn explicit_rediscovery_discards_endpoints_a_fresh_browser_cannot_remove() {
    let files = files();
    let address = "127.0.0.1:47321".parse().unwrap();
    let (mut owner, _, _, _) = trusted_owner(&files, address);
    owner.endpoints.insert(
        "stale-duplicate".into(),
        Endpoint {
            id: CLIENT.into(),
            name: "旧的重复广告".into(),
            address,
        },
    );
    assert_eq!(owner.endpoints.len(), 2);
    owner.command(Command::Discover).unwrap();
    assert!(owner.endpoints.is_empty());
    assert_eq!(owner.selected.as_deref(), Some(CLIENT));
    assert!(owner.records.contains_key(CLIENT));
}

#[test]
fn automatic_resume_does_not_override_a_bonjour_failure_backoff() {
    let files = files();
    let (mut owner, _, _, _) = trusted_owner(&files, "127.0.0.1:47321".parse().unwrap());
    owner.endpoints.clear();
    owner.found(super::super::discovery::Found::Failed(
        "temporary DNS failure".into(),
    ));
    let retry_at = owner.discovery.retry_at().unwrap();
    assert!(owner.connect().is_err());
    assert_eq!(owner.discovery.retry_at(), Some(retry_at));
}
