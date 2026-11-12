use super::*;
use super::{commands::QueuedCommand, owner::Owner};
use loki_metris_pairing::assets::AssetDescriptor;
#[cfg(target_os = "macos")]
use loki_metris_pairing::{SenderSession, session::AwaitingDeviceProof};
use serde_json::{Value, json};
#[cfg(target_os = "macos")]
use std::collections::BTreeMap;
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc::Receiver,
    // 线程
    thread,
    time::{Duration, Instant},
};

// 测试用的桌面 ID
pub(super) const CLIENT: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
// 构造一项完整可用的用量
pub(super) fn amount(total: &str) -> Value {
    json!({"total":total,"availability":"available","completeness":"complete","confidence":"exact"})
}
// 构造一份快照；传入资源时带一条引用该图片的 Hook
pub(super) fn snapshot(assets: Vec<AssetDescriptor>) -> Value {
    json!({"version":1,"tokens":{"day":{"date":"2026-10-26","zone":"UTC","startMs":1,"endMs":100},"observedMs":50,"all":amount("18446744073709551615"),"agents":[{"agent":"codex","amount":amount("0")}]},"tokenError":null,"hooksAvailable":true,"hooks":assets.first().map(|d|vec![json!({"agent":"codex","slot":1,"behavior":"running","content":"真实 TCP 中文📱","contentTruncated":false,"image":d.hash})]).unwrap_or_default(),"assets":assets,"catalogError":null,"enabledAgents":[],"providerQuota":{"observedAtUnix":1,"providers":[]}})
}
// 模拟桌面端发送一帧：4 字节大端长度加 JSON
pub(super) fn server_send(s: &mut TcpStream, v: &Value) {
    let b = serde_json::to_vec(v).unwrap();
    s.write_all(&(b.len() as u32).to_be_bytes()).unwrap();
    s.write_all(&b).unwrap();
}
// 模拟桌面端接收一帧，并检查长度范围
pub(super) fn server_recv(s: &mut TcpStream, max: usize) -> Value {
    let mut size = [0; 4];
    s.read_exact(&mut size).unwrap();
    let size = u32::from_be_bytes(size) as usize;
    assert!(size > 0 && size <= max);
    let mut b = vec![0; size];
    s.read_exact(&mut b).unwrap();
    serde_json::from_slice(&b).unwrap()
}
// 给测试套接字设置读写超时
pub(super) fn configured(s: TcpStream) -> TcpStream {
    s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
    s.set_write_timeout(Some(Duration::from_secs(15))).unwrap();
    s
}
fn view(published: &Published) -> Value {
    serde_json::from_str(&published.state.lock().unwrap().view).unwrap_or(Value::Null)
}
// 轮询已发布的视图直到满足条件，最多等 15 秒，超时则带上状态断言失败
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
        // 休眠 10 毫秒后重试
        thread::sleep(Duration::from_millis(10));
    }
}
// 把命令包装为 owner 事件
pub(super) fn command(command: Command) -> Event {
    Event::Command(QueuedCommand {
        command,
        revision: 0,
    })
}
// 测试文件：根目录、隔离的凭据服务名与设备 ID
pub(super) struct TestFiles {
    pub(super) root: PathBuf,
    pub(super) service: String,
    pub(super) device: String,
}
// 测试文件目录用完删除
impl Drop for TestFiles {
    // 删除钥匙串条目和临时目录
    fn drop(&mut self) {
        let _ = crate::native::credentials::delete(&self.service, &self.device);
        let _ = fs::remove_dir_all(&self.root);
    }
}
// 构造测试 owner 以及事件通道
pub(super) fn owner_for_test(files: &TestFiles) -> (Owner, SyncSender<Event>, Receiver<Event>) {
    let (tx, rx) = mpsc::sync_channel(256);
    let owner = Owner::for_test(
        files.root.clone(),
        &files.device,
        "隔离测试终端📱",
        &files.service,
        tx.clone(),
    );
    // 返回所有者、发送端与接收端
    (owner, tx, rx)
}
// 生成随机 ID 并据此得到隔离的目录、服务名与设备 ID
pub(super) fn files() -> TestFiles {
    let id = uuid::Uuid::new_v4().to_string();
    TestFiles {
        root: std::env::temp_dir().join(format!("loki-link-test-{id}")),
        service: format!("com.manonloki.loki-link-test.{id}"),
        device: id,
    }
}

/// 演练生产环境的所有者和真实的回环套接字,并使用隔离的真实 macOS 钥匙串条目。
/// 测试上报夹具的移动平台;它不断言旧服务会接受桌面端元数据。
#[test]
#[cfg(target_os = "macos")]
// 完整流程：TCP owner 配对、写入钥匙串、恢复、同步图片，并拒绝重放
fn tcp_owner_pairs_commits_keychain_resumes_syncs_assets_and_rejects_replay() {
    // 准备文件与回环监听
    let files = files();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    // 配对码通道：把应用显示的码交给模拟桌面
    let (code_tx, code_rx) = mpsc::sync_channel::<String>(1);
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let device = files.device.clone();
    // 构造 36,111 字节的假 GIF，需要多块传输
    let mut bytes = b"GIF89a".to_vec();
    bytes.resize(36_111, 0x71);
    let descriptor = AssetDescriptor::from_bytes(&bytes).unwrap();
    let expected = bytes.clone();
    let hash = descriptor.hash.clone();
    // 模拟桌面端线程
    let server = thread::spawn(move || {
        let mut socket = configured(listener.accept().unwrap().0);
        // 开场消息必须是配对邀请，带有设备 ID
        let intro = server_recv(&mut socket, HANDSHAKE_FRAME);
        assert_eq!(intro["kind"], "pairing");
        assert_eq!(intro["value"]["deviceId"], device);
        // 等待应用显示的配对码
        let code = code_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        // 用配对码开启发送端会话
        let (sender, hello) =
            SenderSession::start(CLIENT, &device, &code, uuid::Uuid::new_v4().to_string()).unwrap();
        server_send(&mut socket, &serde_json::to_value(hello).unwrap());
        // 应用的回复里带着开窗时的名称
        let reply: loki_metris_pairing::PairingReply =
            serde_json::from_value(server_recv(&mut socket, HANDSHAKE_FRAME)).unwrap();
        assert_eq!(reply.name, "隔离测试终端📱");
        let (sender, confirmation) = sender.accept(reply).unwrap();
        server_send(&mut socket, &serde_json::to_value(confirmation).unwrap());
        // 校验应用的确认，得到共享秘密
        let material = sender
            .verify(serde_json::from_value(server_recv(&mut socket, HANDSHAKE_FRAME)).unwrap())
            .unwrap();
        // 配对完成后连接关闭
        drop(socket);
        // 第二条连接：用保存的秘密恢复
        let mut socket = configured(listener.accept().unwrap().0);
        let intro = server_recv(&mut socket, HANDSHAKE_FRAME);
        assert_eq!(intro["kind"], "resume");
        // 校验设备证明并回复
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
        // 读取设备的首条加密消息
        let info: Value = serde_json::from_slice(
            &channel
                .open(&serde_json::from_value(server_recv(&mut socket, MAX_FRAME)).unwrap())
                .unwrap(),
        )
        .unwrap();
        // 设备 ID 与平台应符合预期
        assert_eq!(info["deviceId"], device);
        assert_eq!(info["platform"], "ios");
        // 推送快照（带图片资源），声明支持快照查询
        let packet=channel.seal(&serde_json::to_vec(&json!({"type":"snapshot","snapshot":snapshot(vec![descriptor.clone()]),"capabilities":["snapshotQueryV1"]})).unwrap()).unwrap();
        server_send(&mut socket, &serde_json::to_value(&packet).unwrap());
        // 按偏移逐块响应图片请求
        let mut offset = 0;
        while offset < descriptor.length {
            let request: Value = serde_json::from_slice(
                &channel
                    .open(&serde_json::from_value(server_recv(&mut socket, MAX_FRAME)).unwrap())
                    .unwrap(),
            )
            .unwrap();
            // 请求必须是对应图片、对应偏移
            assert_eq!(request["type"], "assetRequest");
            assert_eq!(request["hash"], descriptor.hash);
            assert_eq!(request["offset"], offset);
            let chunk = descriptor.chunk(&bytes, offset).unwrap();
            // 每块最多 16 KiB
            offset += (descriptor.length - offset).min(16_384);
            let packet = channel
                .seal(&serde_json::to_vec(&json!({"type":"assetChunk","chunk":chunk})).unwrap())
                .unwrap();
            server_send(&mut socket, &serde_json::to_value(packet).unwrap());
        }
        let ping = channel.seal(br#"{"type":"ping","nonce":7}"#).unwrap();
        server_send(&mut socket, &serde_json::to_value(ping).unwrap());
        // 应用必须回应相同编号的 pong
        let pong: Value = serde_json::from_slice(
            &channel
                .open(&serde_json::from_value(server_recv(&mut socket, MAX_FRAME)).unwrap())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(pong, json!({"type":"pong","nonce":7}));
        // 等测试通知后，重放最初的快照密文：共享通道必须永久关闭
        done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        // 在后续数据包之后重放原始快照密文:共享通道必须永久关闭。
        server_send(&mut socket, &serde_json::to_value(packet).unwrap());
        thread::sleep(Duration::from_millis(300));
    });
    // 应用端：构造 owner，并直接注入发现到的桌面
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
    // 在线程里运行 owner 事件循环
    let worker = thread::spawn(move || owner.run(rx));
    // 选择桌面，开启配对
    tx.send(command(Command::Select {
        client_id: CLIENT.into(),
    }))
    .unwrap();
    let v = until(&published, |v| v["pairingCode"].is_string());
    // 配对途中改名：只影响后续连接
    tx.send(command(Command::Rename {
        name: "新的名称只影响后续连接".into(),
    }))
    .unwrap();
    until(&published, |value| {
        value["deviceName"] == "新的名称只影响后续连接"
    });
    // 把配对码交给模拟桌面
    code_tx
        .send(v["pairingCode"].as_str().unwrap().into())
        .unwrap();
    // 等待连接成功且图片已落盘
    let v = until(&published, |v| {
        v["connected"] == true && v["images"][&hash].is_string()
    });
    // 快照里的总量保持 u64 最大值
    assert_eq!(
        v["snapshot"]["tokens"]["all"]["total"],
        "18446744073709551615"
    );
    assert_eq!(v["live"], true);
    assert!(v["paired"][CLIENT].is_object());
    // 公开视图不含任何秘密
    assert!(v.to_string().find("secret").is_none());
    // 图片文件内容与原始字节一致
    assert_eq!(
        fs::read(files.root.join("assets").join(&hash)).unwrap(),
        expected
    );
    // 手表发件箱里的清单同步更新
    let companion: Value =
        serde_json::from_slice(&fs::read(files.root.join("companion-v1/manifest.json")).unwrap())
            .unwrap();
    assert_eq!(companion["sourceId"], CLIENT);
    assert_eq!(companion["receivedAtMs"], v["receivedAtMs"]);
    assert_eq!(
        companion["snapshot"]["tokens"]["all"]["total"],
        "18446744073709551615"
    );
    // 手表清单里不含本地图片地址
    assert!(
        companion["snapshot"]["hooks"][0]
            .get("imageSource")
            .is_none()
    );
    // 手表发件箱里的图片内容也一致
    assert_eq!(
        fs::read(files.root.join("companion-v1/assets").join(&hash)).unwrap(),
        expected
    );
    // 钥匙串中已保存配对记录
    let stored = crate::native::credentials::read(&files.service, &files.device)
        .unwrap()
        .unwrap();
    let records: BTreeMap<String, Material> = serde_json::from_str(&stored).unwrap();
    assert_eq!(records[CLIENT].device_id, files.device);
    // 规范的私有缓存在断开期间保留来源时间戳,并恢复已验证的图片。
    // 离线缓存：保留来源的接收时间，并恢复已校验的图片
    let (mut cached, _cached_tx, _cached_rx) = owner_for_test(&files);
    cached.records = records;
    cached.restore_cache();
    assert!(cached.view["images"][&hash].is_string());
    assert_eq!(cached.view["live"], false);
    assert_eq!(cached.view["receivedAtMs"], v["receivedAtMs"]);
    // 遗忘已缓存的来源会清除所有内容,桌面端也一样。
    // 忘记被缓存的来源：即使在桌面端也清空所有内容
    cached.selected = Some(CLIENT.into());
    cached
        .command(Command::Forget {
            client_id: CLIENT.into(),
        })
        .unwrap();
    assert!(cached.view["snapshot"].is_null());
    assert_eq!(cached.view["images"], json!({}));
    // 通知模拟桌面开始重放
    done_tx.send(()).unwrap();
    // 重放被拒绝，连接断开并带有错误
    until(&published, |v| {
        v["connected"] == false && !v["error"].is_null()
    });
    // 停止 owner，等待两个线程结束
    tx.send(command(Command::Stop)).unwrap();
    worker.join().unwrap();
    server.join().unwrap();
}

#[test]
fn a_view_subscription_wakes_only_for_new_publications_or_close() {
    use std::{
        future::Future,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll, Wake, Waker},
    };

    #[derive(Default)]
    struct Notifications(AtomicUsize);

    impl Wake for Notifications {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    let published = super::Published::default();
    published.update("initial".into());
    let notifications = Arc::new(Notifications::default());
    let waker = Waker::from(notifications.clone());
    let mut cx = Context::from_waker(&waker);
    let mut change = std::pin::pin!(published.wait_for_change("initial"));
    assert_eq!(change.as_mut().poll(&mut cx), Poll::Pending);

    published.update("initial".into());
    assert_eq!(notifications.0.load(Ordering::Relaxed), 0);
    published.update("next".into());
    assert_eq!(notifications.0.load(Ordering::Relaxed), 1);
    assert_eq!(change.as_mut().poll(&mut cx), Poll::Ready(true));

    let mut close = std::pin::pin!(published.wait_for_change("next"));
    assert_eq!(close.as_mut().poll(&mut cx), Poll::Pending);
    published.close();
    assert_eq!(notifications.0.load(Ordering::Relaxed), 2);
    assert_eq!(close.as_mut().poll(&mut cx), Poll::Ready(false));
}
