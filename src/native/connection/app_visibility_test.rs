// 引入 owner 与命令等
use super::super::{
    Command, Endpoint, Event, HANDSHAKE_FRAME, HostCommand, MAX_FRAME, Material,
    connection_test::{
        CLIENT, TestFiles, command, configured, files, owner_for_test, server_recv, server_send,
        snapshot, until,
    },
    owner::Owner,
    persistence,
};
// 引入配对会话类型
use loki_metris_pairing::session::AwaitingDeviceProof;
use serde_json::{Value, json};
use std::{
    io::Read,
    net::{TcpListener, TcpStream},
    sync::mpsc::{Receiver, SyncSender},
    thread,
};

/// 已配对桌面：以保存的秘密恢复认证、交付一份快照，返回服务端加密通道。
// 驱动一次恢复并投递快照
fn resume_and_snapshot(
    listener: &TcpListener,
    device: &str,
    secret: &[u8; 32],
    capabilities: Value,
    endpoint: Value,
) -> (TcpStream, loki_metris_pairing::session::SecureChannel) {
    // 接受连接
    let mut socket = configured(listener.accept().unwrap().0);
    // 开场消息必须是恢复
    let intro = server_recv(&mut socket, HANDSHAKE_FRAME);
    assert_eq!(intro["kind"], "resume");
    // 校验证明并回复
    let (waiting, reply) = AwaitingDeviceProof::accept(
        serde_json::from_value(intro["value"].clone()).unwrap(),
        CLIENT,
        device,
        secret,
        uuid::Uuid::new_v4().to_string(),
    )
    .unwrap();
    // 服务端发送回复
    server_send(&mut socket, &serde_json::to_value(reply).unwrap());
    // 得到加密通道
    let mut channel = waiting
        .verify(serde_json::from_value(server_recv(&mut socket, HANDSHAKE_FRAME)).unwrap())
        .unwrap();
    // 读掉设备的首条消息
    let _info = channel
        .open(&serde_json::from_value(server_recv(&mut socket, MAX_FRAME)).unwrap())
        .unwrap();
    // 推送快照，带上能力与小组件端点
    let packet = channel
        .seal(&serde_json::to_vec(&json!({"type":"snapshot","snapshot":snapshot(vec![]),"capabilities":capabilities,"widgetEndpoint":endpoint})).unwrap())
        .unwrap();
    server_send(&mut socket, &serde_json::to_value(packet).unwrap());
    (socket, channel)
}

// 构造已信任一台桌面的 owner
fn trusted_owner(
    files: &TestFiles,
    address: std::net::SocketAddr,
) -> (Owner, SyncSender<Event>, Receiver<Event>, [u8; 32]) {
    let (mut owner, tx, rx) = owner_for_test(files);
    let secret = [7u8; 32];
    // 登记配对记录
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
    // 选中并注入发现到的端点
    owner.selected = Some(CLIENT.into());
    owner.endpoints.insert(
        "fixture".into(),
        Endpoint {
            id: CLIENT.into(),
            name: "环回验证台".into(),
            address,
        },
    );
    // 返回所有者、通道与密钥
    (owner, tx, rx, secret)
}

#[test]
// 离开前台先发 pause 再关闭，并且设备形态只上报一次
fn leaving_the_foreground_sends_pause_then_closes_and_reports_form_factor_once() {
    let files = files();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (mut owner, tx, rx, secret) = trusted_owner(&files, listener.local_addr().unwrap());
    // 本机是手机形态
    owner.form_factor = Some("phone".into());
    let published = owner.published.clone();
    let worker = thread::spawn(move || owner.run(rx));
    // 刷新：触发恢复连接
    tx.send(command(Command::Refresh)).unwrap();
    let device = files.device.clone();
    // 桌面声明支持设备形态上报
    let (mut socket, mut channel) = resume_and_snapshot(
        &listener,
        &device,
        &secret,
        json!(["deviceFormFactorV1"]),
        Value::Null,
    );
    // 读取并解密桌面收到的下一条消息
    let mut next = |socket: &mut TcpStream| -> Value {
        serde_json::from_slice(
            &channel
                .open(&serde_json::from_value(server_recv(socket, MAX_FRAME)).unwrap())
                .unwrap(),
        )
        .unwrap()
    };
    // 应用首先上报一次设备形态
    assert_eq!(
        next(&mut socket),
        json!({"type":"deviceFormFactor","formFactor":"phone"})
    );
    // 等待已连接
    until(&published, |v| v["connected"] == true);
    // 应用离开前台
    tx.send(command(Command::Host(HostCommand::Lifecycle {
        visible: false,
        permitted: true,
    })))
    .unwrap();
    // 桌面应先收到 pause
    assert_eq!(next(&mut socket), json!({"type":"pause"}));
    // 之后连接关闭，不再有任何帧
    let mut rest = Vec::new();
    socket.read_to_end(&mut rest).unwrap();
    assert!(
        rest.is_empty(),
        "the paused stream must close without further frames"
    );
    // 视图变为未连接，状态为“已暂停”
    let v = until(&published, |v| v["connected"] == false);
    assert_eq!(v["status"], "已暂停，回到前台后恢复");
    tx.send(command(Command::Stop)).unwrap();
    worker.join().unwrap();
}

#[test]
#[cfg(target_os = "macos")]
// 已退役的配对簿不能被导入，也不会阻止新建
fn a_retired_pairing_book_cannot_be_imported_or_block_a_new_book() {
    let files = files();
    // 旧版的凭据服务名
    let retired = "com.manonloki.lokimetris.display.pairing.v1";
    // 在旧命名空间里写入一份无效记录
    crate::native::credentials::write(retired, &files.device, "retired, invalid records").unwrap();
    // 读取当前命名空间
    let result = persistence::load_records(super::super::CREDENTIAL_SERVICE, &files.device);
    crate::native::credentials::delete(retired, &files.device).unwrap();
    // 当前配对簿为空，不读取旧记录
    assert!(
        result.unwrap().is_empty(),
        "a fresh NSH pairing book must not read retired records"
    );
}

#[test]
// 显式重新发现会丢弃新浏览器无法移除的旧端点
fn explicit_rediscovery_discards_endpoints_a_fresh_browser_cannot_remove() {
    let files = files();
    let address = "127.0.0.1:47321".parse().unwrap();
    let (mut owner, _, _, _) = trusted_owner(&files, address);
    // 再加一个重复声明同一身份的陈旧端点
    owner.endpoints.insert(
        "stale-duplicate".into(),
        Endpoint {
            id: CLIENT.into(),
            name: "旧的重复广告".into(),
            address,
        },
    );
    // 应有两个端点
    assert_eq!(owner.endpoints.len(), 2);
    // 显式重新发现
    owner.command(Command::Discover).unwrap();
    // 所有端点被清空（新浏览器自己会补回真实的）
    assert!(owner.endpoints.is_empty());
    // 选择与配对记录不受影响
    assert_eq!(owner.selected.as_deref(), Some(CLIENT));
    assert!(owner.records.contains_key(CLIENT));
}

#[test]
// 自动恢复不会覆盖 Bonjour 失败后的退避
fn automatic_resume_does_not_override_a_bonjour_failure_backoff() {
    let files = files();
    let (mut owner, _, _, _) = trusted_owner(&files, "127.0.0.1:47321".parse().unwrap());
    // 没有任何端点
    owner.endpoints.clear();
    // 模拟 Bonjour 失败，进入退避
    owner.found(super::super::discovery::Found::Failed(
        "temporary DNS failure".into(),
    ));
    let retry_at = owner.discovery.retry_at().unwrap();
    // 自动重连失败
    assert!(owner.connect().is_err());
    // 且退避时间没有被改写
    assert_eq!(owner.discovery.retry_at(), Some(retry_at));
}
