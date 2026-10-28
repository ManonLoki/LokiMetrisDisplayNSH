//! 隔离的桌面夹具,使用实际的共享配对与加密传输。
//! 用 `cargo run --example protocol_host` 启动;出现提示时输入手机上的四位数代码。
//! 配对只存在于内存中,进程停止即消失。不会打开任何生产设备簿。
// 引入真实的配对与加密传输库
use loki_metris_pairing::{
    SenderSession,
    assets::AssetDescriptor,
    session::{AwaitingDeviceProof, SecureChannel},
    valid_device_id, valid_device_name,
};
// 引入 mDNS 守护进程，用于广播服务
use mdns_sd::{IfKind, ServiceDaemon, ServiceInfo};
// 引入 JSON
use serde_json::{Value, json};
// 引入集合、IO、网络、同步、线程与时间
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    // 线程
    thread,
    // 时长与时刻
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
// 夹具桌面的默认设备 ID
const CLIENT: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
// 默认传输的测试图片
const PICTURE: &[u8] = include_bytes!("../../assets/branding/app-icon.png");
// 发送一帧：4 字节大端长度前缀加 JSON
fn send(socket: &mut TcpStream, value: &Value) -> io::Result<()> {
    let b = serde_json::to_vec(value)?;
    socket.write_all(&(b.len() as u32).to_be_bytes())?;
    socket.write_all(&b)
}
// 接收一帧，长度为零或超过上限则报错
fn recv(socket: &mut TcpStream, max: usize) -> io::Result<Value> {
    let mut n = [0; 4];
    socket.read_exact(&mut n)?;
    let size = u32::from_be_bytes(n) as usize;
    if size == 0 || size > max {
        return Err(io::Error::other("frame limit"));
    }
    let mut b = vec![0; size];
    // 读满帧体
    socket.read_exact(&mut b)?;
    // 解析为 JSON
    Ok(serde_json::from_slice(&b)?)
}
// 加密后发送
fn secure_send(
    s: &mut TcpStream,
    c: &mut SecureChannel,
    v: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    send(s, &serde_json::to_value(c.seal(&serde_json::to_vec(v)?)?)?)?;
    Ok(())
}
// 构造测试快照：含 Token、四条 Hook、额度和图片资源
fn snapshot(picture: &[u8]) -> Value {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    // 当天的起点（毫秒）
    let start = now / 86_400_000 * 86_400_000;
    let asset = AssetDescriptor::from_bytes(picture).unwrap();
    let amount = |v: &str| json!({"total":v,"availability":"available","completeness":"complete","confidence":"exact"});
    json!({"type":"snapshot","capabilities":["snapshotQueryV1","deviceFormFactorV1"],"snapshot":{
  "version":1,"tokens":{"day":{"date":"2026-10-26","zone":"UTC","startMs":start,"endMs":start+86_400_000},"observedMs":now,"all":amount("18446744073709551615"),"agents":[{"agent":"codex","amount":amount("1256789")},{"agent":"claudeCode","amount":amount("9876543")}]},
  "tokenError":null,"hooksAvailable":true,"hooks":[
   {"agent":"codex","slot":1,"behavior":"running","content":"真实加密 TCP · 多块图片传输验证","contentTruncated":false,"image":asset.hash},
   {"agent":"claudeCode","slot":2,"behavior":"asking","content":"等待确认 · 协议测试数据","contentTruncated":false,"image":null},
   // Hook 样例：Grok 空闲
   {"agent":"grok","slot":3,"behavior":"idle","content":"空闲 · 离线缓存验证","contentTruncated":false,"image":null},
   // Hook 样例：Cursor 错误
   {"agent":"cursor","slot":4,"behavior":"error","content":"错误状态展示验证","contentTruncated":false,"image":null}],
  // 资源、目录错误与启用名单
  "assets":[asset],"catalogError":null,"enabledAgents":["codex","claudeCode","cursor","grok"],
  // 额度观测时间与供应商
  "providerQuota":{"observedAtUnix":now/1000,"providers":[
   // Codex 套餐与窗口
   {"provider":"codex","plan":"Plus","fiveHour":{"kind":"fiveHour","usedTenths":250},"weekly":{"kind":"weekly","usedTenths":500}},
   // Claude 套餐与每周窗口
   {"provider":"claude","plan":"Max","weekly":{"kind":"weekly","usedTenths":1000}},
   // Cursor 套餐与每月窗口
   {"provider":"cursor","plan":"Pro","monthly":{"kind":"monthly","usedTenths":100}},
   // Grok 五小时窗口
   {"provider":"grok","fiveHour":{"kind":"fiveHour","usedTenths":0}}]}}})
}
// 处理一个连接：先配对或恢复，再推送快照并响应请求
fn serve(
    mut s: TcpStream,
    records: Arc<Mutex<BTreeMap<String, [u8; 32]>>>,
    input: Arc<Mutex<()>>,
    config: Arc<Config>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 读写超时
    s.set_read_timeout(Some(Duration::from_secs(65)))?;
    s.set_write_timeout(Some(Duration::from_secs(15)))?;
    // 读取开场消息
    let intro = recv(&mut s, 4096)?;
    let device = intro["value"]["deviceId"]
        .as_str()
        .ok_or("device identity")?
        .to_owned();
    // 首次配对：要求在终端输入应用显示的四位码
    if intro["kind"] == "pairing" {
        if intro["value"]["clientId"] != config.client_id {
            return Err("client identity".into());
        }
        // 同一时间只提示一个配对
        let _lock = input.lock().unwrap();
        print!("PAIRING {device}: enter four-digit code shown in app: ");
        io::stdout().flush()?;
        let mut code = String::new();
        io::stdin().read_line(&mut code)?;
        // 用配对码开启发送端会话
        let (sender, hello) = SenderSession::start(
            &config.client_id,
            &device,
            code.trim(),
            uuid::Uuid::new_v4().to_string(),
        )?;
        send(&mut s, &serde_json::to_value(hello)?)?;
        let (sender, confirmation) = sender.accept(serde_json::from_value(recv(&mut s, 4096)?)?)?;
        // 发送确认消息
        send(&mut s, &serde_json::to_value(confirmation)?)?;
        // 校验对端的确认
        let material = sender.verify(serde_json::from_value(recv(&mut s, 4096)?)?)?;
        // 配对秘密只保存在内存里
        let mut saved = records.lock().unwrap();
        // 夹具最多保存 64 条
        if saved.len() >= 64 && !saved.contains_key(&device) {
            return Err("fixture pairing limit".into());
        }
        saved.insert(device.clone(), *material.secret_bytes());
        drop(saved);
        println!("PAIRED {device} (secret retained in fixture memory only)");
        return Ok(());
    }
    // 其余必须是恢复
    if intro["kind"] != "resume" {
        return Err("introduction".into());
    }
    // 取出该设备已保存的秘密，未知设备要求重新配对
    let secret = *records
        .lock()
        .unwrap()
        .get(&device)
        .ok_or("unknown device; restart pairing")?;
    // 校验设备的证明并回复
    let (waiting, reply) = AwaitingDeviceProof::accept(
        serde_json::from_value(intro["value"].clone())?,
        &config.client_id,
        &device,
        &secret,
        uuid::Uuid::new_v4().to_string(),
    )?;
    send(&mut s, &serde_json::to_value(reply)?)?;
    // 得到加密通道
    let mut channel = waiting.verify(serde_json::from_value(recv(&mut s, 4096)?)?)?;
    // 读取设备的首条加密消息
    let packet = serde_json::from_value(recv(&mut s, 96 * 1024)?)?;
    let info: Value = serde_json::from_slice(&channel.open(&packet)?)?;
    // 平台必须是已知值
    let platform = info["platform"].as_str().ok_or("platform")?;
    // 夹具接受桌面端的值,以演练提议中的服务端扩展。
    // 夹具接受桌面平台，以验证服务端的扩展
    if !["ios", "android", "macos", "windows", "linux"].contains(&platform) {
        return Err("platform".into());
    }
    println!("AUTHENTICATED {device} platform={platform}");
    // 认证后立即推送快照
    let message = snapshot(&config.picture);
    secure_send(&mut s, &mut channel, &message)?;
    // 之后改用短超时轮询
    s.set_read_timeout(Some(Duration::from_millis(100)))?;
    let asset = AssetDescriptor::from_bytes(&config.picture)?;
    // 心跳编号
    let mut nonce = 0u64;
    let mut last_ping = Instant::now();
    let mut last_received = Instant::now();
    let mut reader = FrameReader::default();
    loop {
        // 每 15 秒发一次心跳
        if last_ping.elapsed() >= Duration::from_secs(15) {
            nonce += 1;
            secure_send(&mut s, &mut channel, &json!({"type":"ping","nonce":nonce}))?;
            last_ping = Instant::now();
        }
        // 45 秒没有收到数据则视为心跳过期
        if last_received.elapsed() >= Duration::from_secs(45) {
            return Err("inbound heartbeat expired".into());
        }
        // 没有完整帧就继续轮询
        let Some(packet) = reader.poll(&mut s)? else {
            continue;
        };
        last_received = Instant::now();
        // 解密收到的消息
        let message: Value =
            serde_json::from_slice(&channel.open(&serde_json::from_value(packet)?)?)?;
        match message["type"].as_str() {
            // 图片请求：按偏移返回一块
            Some("assetRequest") => {
                if message["hash"] != asset.hash {
                    return Err("asset hash".into());
                }
                let offset = message["offset"].as_u64().ok_or("asset offset")?;
                secure_send(
                    &mut s,
                    &mut channel,
                    // 发送资源分块
                    &json!({"type":"assetChunk","chunk":asset.chunk(&config.picture,offset)?}),
                )?;
                // 最后一块发完
                if offset + 16_384 >= asset.length {
                    println!("ASSET_COMPLETE {device} bytes={}", asset.length);
                }
            }
            // 快照请求：重新推送
            Some("snapshotRequest") => {
                let message = snapshot(&config.picture);
                secure_send(&mut s, &mut channel, &message)?
            }
            // 心跳回应与设备形态无需处理
            Some("pong") | Some("deviceFormFactor") => {}
            // 设备暂停：结束本连接
            Some("pause") => {
                println!("PAUSED {device}");
                return Ok(());
            }
            _ => return Err("message type".into()),
        }
    }
}
/// 在较短的轮询超时之间,保留不完整的 TCP 头部/正文。
#[derive(Default)]
// 帧读取器：在短超时之间保留不完整的 TCP 头与正文
struct FrameReader {
    bytes: Vec<u8>,
    partial_since: Option<Instant>,
}
impl FrameReader {
    // 轮询：有完整帧则返回；否则尝试读取更多数据
    fn poll(&mut self, socket: &mut TcpStream) -> io::Result<Option<Value>> {
        // 不完整帧超过 15 秒视为超时
        if self
            .partial_since
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(15))
        {
            return Err(io::Error::other("partial frame expired"));
        }
        // 已有长度前缀
        if self.bytes.len() >= 4 {
            let size = u32::from_be_bytes(self.bytes[..4].try_into().unwrap()) as usize;
            if size == 0 || size > 96 * 1024 {
                return Err(io::Error::other("frame limit"));
            }
            // 完整帧：解析并移除
            if self.bytes.len() >= size + 4 {
                let value = serde_json::from_slice(&self.bytes[4..size + 4])?;
                self.bytes.drain(..size + 4);
                self.partial_since = if self.bytes.is_empty() {
                    None
                } else {
                    Some(Instant::now())
                };
                // 返回解析出的值
                return Ok(Some(value));
            }
        }
        // 读缓冲区
        let mut chunk = [0; 8192];
        // 读取更多数据
        match socket.read(&mut chunk) {
            // 对端关闭
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "connection closed",
                ));
            }
            Ok(n) => {
                if self.bytes.is_empty() {
                    // 记下开始接收不完整帧的时刻
                    self.partial_since = Some(Instant::now());
                }
                // 追加读到的字节
                self.bytes.extend_from_slice(&chunk[..n]);
            }
            // 超时只表示暂时没有数据
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e),
        }
        // 缓冲区超限
        if self.bytes.len() > 96 * 1024 + 4 {
            return Err(io::Error::other("frame limit"));
        }
        Ok(None)
    }
}
// 夹具配置
struct Config {
    client_id: String,
    name: String,
    picture: Vec<u8>,
}
impl Config {
    // 解析命令行参数 --id、--name、--asset
    fn parse() -> Result<Self, Box<dyn std::error::Error>> {
        // 默认值
        let mut result = Self {
            client_id: CLIENT.into(),
            name: "Loki 协议验证台".into(),
            picture: PICTURE.to_vec(),
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                // 客户端标识
                "--id" => result.client_id = args.next().ok_or("missing --id value")?,
                // 设备名
                "--name" => result.name = args.next().ok_or("missing --name value")?,
                // 自定义图片不得超过 32 MiB
                "--asset" => {
                    let path = args.next().ok_or("missing --asset value")?;
                    if std::fs::metadata(&path)?.len() > 32 * 1024 * 1024 {
                        return Err("asset too large".into());
                    }
                    result.picture = std::fs::read(path)?;
                }
                // 未知参数：给出用法
                _ => {
                    return Err(
                        "usage: protocol_host [--id UUID] [--name NAME] [--asset IMAGE]".into(),
                    );
                }
            }
        }
        // 身份必须合法
        if !valid_device_id(&result.client_id) || !valid_device_name(&result.name) {
            return Err("invalid fixture identity".into());
        }
        // 图片必须能生成合法的资源描述
        AssetDescriptor::from_bytes(&result.picture)?;
        Ok(result)
    }
}
// 启动夹具：监听端口、广播 mDNS 服务，并为每个连接开线程
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Arc::new(Config::parse()?);
    let listener = TcpListener::bind("0.0.0.0:0")?;
    let port = listener.local_addr()?.port();
    let mdns = ServiceDaemon::new()?;
    // 只在 IPv4 的非回环网卡上广播
    mdns.disable_interface(vec![IfKind::IPv6, IfKind::LoopbackV4])?;
    // TXT 记录：协议版本、设备 ID 与名称
    let properties = [
        ("v", "1"),
        ("id", config.client_id.as_str()),
        ("name", config.name.as_str()),
    ];
    let hostname = format!("loki-fixture-{}.local.", config.client_id);
    let info = ServiceInfo::new(
        "_loki-host._tcp.local.",
        // 实例名
        &config.name,
        // 主机名
        &hostname,
        // 空 IP，自动取地址
        "",
        // 端口
        port,
        // TXT 属性
        &properties[..],
    )?
    // 自动检测地址
    .enable_addr_auto();
    // 注册服务
    mdns.register(info)?;
    // 输出就绪标志，测试脚本据此继续
    println!(
        // 就绪信息格式
        "FIXTURE_READY port={port} clientId={} name={} assetBytes={}; test data only.",
        // 客户端标识
        config.client_id,
        // 设备名
        config.name,
        // 图片字节数
        config.picture.len()
    );
    // 内存中的配对簿，进程结束即消失
    let records = Arc::new(Mutex::new(BTreeMap::new()));
    // 终端输入锁
    let input = Arc::new(Mutex::new(()));
    // 当前连接数
    let connections = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        // 连接许可：离开作用域时归还名额
        struct Permit(Arc<AtomicUsize>);
        impl Drop for Permit {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        // 最多同时 72 个连接，超出直接丢弃
        if connections.fetch_add(1, Ordering::SeqCst) >= 72 {
            connections.fetch_sub(1, Ordering::SeqCst);
            drop(stream);
            continue;
        }
        let permit = Permit(connections.clone());
        let r = records.clone();
        let i = input.clone();
        // 克隆配置给线程
        let c = config.clone();
        // 每个连接一个线程
        thread::spawn(move || {
            let _permit = permit;
            if let Err(e) = serve(stream.unwrap(), r, i, c) {
                eprintln!("Fixture connection ended: {e}");
            }
        });
    }
    Ok(())
}

// 测试模块
#[cfg(test)]
#[path = "protocol_host_test.rs"]
mod protocol_host_test;
