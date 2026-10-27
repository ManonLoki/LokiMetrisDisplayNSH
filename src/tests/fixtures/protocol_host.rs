//! Isolated desktop fixture using the actual shared pairing and encrypted transport.
//! Start with `cargo run --example protocol_host`; type the phone's four-digit code when prompted.
//! Pairings are memory-only and disappear when this process stops. No production device book is opened.
use loki_metris_pairing::{
    SenderSession,
    assets::AssetDescriptor,
    session::{AwaitingDeviceProof, SecureChannel},
    valid_device_id, valid_device_name,
};
use mdns_sd::{IfKind, ServiceDaemon, ServiceInfo};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const CLIENT: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const PICTURE: &[u8] = include_bytes!("../../assets/branding/app-icon.png");
fn send(socket: &mut TcpStream, value: &Value) -> io::Result<()> {
    let b = serde_json::to_vec(value)?;
    socket.write_all(&(b.len() as u32).to_be_bytes())?;
    socket.write_all(&b)
}
fn recv(socket: &mut TcpStream, max: usize) -> io::Result<Value> {
    let mut n = [0; 4];
    socket.read_exact(&mut n)?;
    let size = u32::from_be_bytes(n) as usize;
    if size == 0 || size > max {
        return Err(io::Error::other("frame limit"));
    }
    let mut b = vec![0; size];
    socket.read_exact(&mut b)?;
    Ok(serde_json::from_slice(&b)?)
}
fn secure_send(
    s: &mut TcpStream,
    c: &mut SecureChannel,
    v: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    send(s, &serde_json::to_value(c.seal(&serde_json::to_vec(v)?)?)?)?;
    Ok(())
}
fn snapshot(picture: &[u8]) -> Value {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let start = now / 86_400_000 * 86_400_000;
    let asset = AssetDescriptor::from_bytes(picture).unwrap();
    let amount = |v: &str| json!({"total":v,"availability":"available","completeness":"complete","confidence":"exact"});
    json!({"type":"snapshot","capabilities":["snapshotQueryV1","deviceFormFactorV1"],"snapshot":{
  "version":1,"tokens":{"day":{"date":"2026-10-26","zone":"UTC","startMs":start,"endMs":start+86_400_000},"observedMs":now,"all":amount("18446744073709551615"),"agents":[{"agent":"codex","amount":amount("1256789")},{"agent":"claudeCode","amount":amount("9876543")}]},
  "tokenError":null,"hooksAvailable":true,"hooks":[
   {"agent":"codex","slot":1,"behavior":"running","content":"真实加密 TCP · 多块图片传输验证","contentTruncated":false,"image":asset.hash},
   {"agent":"claudeCode","slot":2,"behavior":"asking","content":"等待确认 · 协议测试数据","contentTruncated":false,"image":null},
   {"agent":"grok","slot":3,"behavior":"idle","content":"空闲 · 离线缓存验证","contentTruncated":false,"image":null},
   {"agent":"cursor","slot":4,"behavior":"error","content":"错误状态展示验证","contentTruncated":false,"image":null}],
  "assets":[asset],"catalogError":null,"enabledAgents":["codex","claudeCode","cursor","grok"],
  "providerQuota":{"observedAtUnix":now/1000,"providers":[
   {"provider":"codex","plan":"Plus","fiveHour":{"kind":"fiveHour","usedTenths":250},"weekly":{"kind":"weekly","usedTenths":500}},
   {"provider":"claude","plan":"Max","weekly":{"kind":"weekly","usedTenths":1000}},
   {"provider":"cursor","plan":"Pro","monthly":{"kind":"monthly","usedTenths":100}},
   {"provider":"grok","fiveHour":{"kind":"fiveHour","usedTenths":0}}]}}})
}
fn serve(
    mut s: TcpStream,
    records: Arc<Mutex<BTreeMap<String, [u8; 32]>>>,
    input: Arc<Mutex<()>>,
    config: Arc<Config>,
) -> Result<(), Box<dyn std::error::Error>> {
    s.set_read_timeout(Some(Duration::from_secs(65)))?;
    s.set_write_timeout(Some(Duration::from_secs(15)))?;
    let intro = recv(&mut s, 4096)?;
    let device = intro["value"]["deviceId"]
        .as_str()
        .ok_or("device identity")?
        .to_owned();
    if intro["kind"] == "pairing" {
        if intro["value"]["clientId"] != config.client_id {
            return Err("client identity".into());
        }
        let _lock = input.lock().unwrap();
        print!("PAIRING {device}: enter four-digit code shown in app: ");
        io::stdout().flush()?;
        let mut code = String::new();
        io::stdin().read_line(&mut code)?;
        let (sender, hello) = SenderSession::start(
            &config.client_id,
            &device,
            code.trim(),
            uuid::Uuid::new_v4().to_string(),
        )?;
        send(&mut s, &serde_json::to_value(hello)?)?;
        let (sender, confirmation) = sender.accept(serde_json::from_value(recv(&mut s, 4096)?)?)?;
        send(&mut s, &serde_json::to_value(confirmation)?)?;
        let material = sender.verify(serde_json::from_value(recv(&mut s, 4096)?)?)?;
        let mut saved = records.lock().unwrap();
        if saved.len() >= 64 && !saved.contains_key(&device) {
            return Err("fixture pairing limit".into());
        }
        saved.insert(device.clone(), *material.secret_bytes());
        drop(saved);
        println!("PAIRED {device} (secret retained in fixture memory only)");
        return Ok(());
    }
    if intro["kind"] != "resume" {
        return Err("introduction".into());
    }
    let secret = *records
        .lock()
        .unwrap()
        .get(&device)
        .ok_or("unknown device; restart pairing")?;
    let (waiting, reply) = AwaitingDeviceProof::accept(
        serde_json::from_value(intro["value"].clone())?,
        &config.client_id,
        &device,
        &secret,
        uuid::Uuid::new_v4().to_string(),
    )?;
    send(&mut s, &serde_json::to_value(reply)?)?;
    let mut channel = waiting.verify(serde_json::from_value(recv(&mut s, 4096)?)?)?;
    let packet = serde_json::from_value(recv(&mut s, 96 * 1024)?)?;
    let info: Value = serde_json::from_slice(&channel.open(&packet)?)?;
    let platform = info["platform"].as_str().ok_or("platform")?;
    // The fixture accepts desktop values to exercise the proposed service-side extension.
    if !["ios", "android", "macos", "windows", "linux"].contains(&platform) {
        return Err("platform".into());
    }
    println!("AUTHENTICATED {device} platform={platform}");
    let message = snapshot(&config.picture);
    secure_send(&mut s, &mut channel, &message)?;
    s.set_read_timeout(Some(Duration::from_millis(100)))?;
    let asset = AssetDescriptor::from_bytes(&config.picture)?;
    let mut nonce = 0u64;
    let mut last_ping = Instant::now();
    let mut last_received = Instant::now();
    let mut reader = FrameReader::default();
    loop {
        if last_ping.elapsed() >= Duration::from_secs(15) {
            nonce += 1;
            secure_send(&mut s, &mut channel, &json!({"type":"ping","nonce":nonce}))?;
            last_ping = Instant::now();
        }
        if last_received.elapsed() >= Duration::from_secs(45) {
            return Err("inbound heartbeat expired".into());
        }
        let Some(packet) = reader.poll(&mut s)? else {
            continue;
        };
        last_received = Instant::now();
        let message: Value =
            serde_json::from_slice(&channel.open(&serde_json::from_value(packet)?)?)?;
        match message["type"].as_str() {
            Some("assetRequest") => {
                if message["hash"] != asset.hash {
                    return Err("asset hash".into());
                }
                let offset = message["offset"].as_u64().ok_or("asset offset")?;
                secure_send(
                    &mut s,
                    &mut channel,
                    &json!({"type":"assetChunk","chunk":asset.chunk(&config.picture,offset)?}),
                )?;
                if offset + 16_384 >= asset.length {
                    println!("ASSET_COMPLETE {device} bytes={}", asset.length);
                }
            }
            Some("snapshotRequest") => {
                let message = snapshot(&config.picture);
                secure_send(&mut s, &mut channel, &message)?
            }
            Some("pong") | Some("deviceFormFactor") => {}
            Some("pause") => {
                println!("PAUSED {device}");
                return Ok(());
            }
            _ => return Err("message type".into()),
        }
    }
}
/// Preserve partial TCP headers/bodies across short polling timeouts.
#[derive(Default)]
struct FrameReader {
    bytes: Vec<u8>,
    partial_since: Option<Instant>,
}
impl FrameReader {
    fn poll(&mut self, socket: &mut TcpStream) -> io::Result<Option<Value>> {
        if self
            .partial_since
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(15))
        {
            return Err(io::Error::other("partial frame expired"));
        }
        if self.bytes.len() >= 4 {
            let size = u32::from_be_bytes(self.bytes[..4].try_into().unwrap()) as usize;
            if size == 0 || size > 96 * 1024 {
                return Err(io::Error::other("frame limit"));
            }
            if self.bytes.len() >= size + 4 {
                let value = serde_json::from_slice(&self.bytes[4..size + 4])?;
                self.bytes.drain(..size + 4);
                self.partial_since = if self.bytes.is_empty() {
                    None
                } else {
                    Some(Instant::now())
                };
                return Ok(Some(value));
            }
        }
        let mut chunk = [0; 8192];
        match socket.read(&mut chunk) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "connection closed",
                ));
            }
            Ok(n) => {
                if self.bytes.is_empty() {
                    self.partial_since = Some(Instant::now());
                }
                self.bytes.extend_from_slice(&chunk[..n]);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e),
        }
        if self.bytes.len() > 96 * 1024 + 4 {
            return Err(io::Error::other("frame limit"));
        }
        Ok(None)
    }
}
struct Config {
    client_id: String,
    name: String,
    picture: Vec<u8>,
}
impl Config {
    fn parse() -> Result<Self, Box<dyn std::error::Error>> {
        let mut result = Self {
            client_id: CLIENT.into(),
            name: "Loki 协议验证台".into(),
            picture: PICTURE.to_vec(),
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--id" => result.client_id = args.next().ok_or("missing --id value")?,
                "--name" => result.name = args.next().ok_or("missing --name value")?,
                "--asset" => {
                    let path = args.next().ok_or("missing --asset value")?;
                    if std::fs::metadata(&path)?.len() > 32 * 1024 * 1024 {
                        return Err("asset too large".into());
                    }
                    result.picture = std::fs::read(path)?;
                }
                _ => {
                    return Err(
                        "usage: protocol_host [--id UUID] [--name NAME] [--asset IMAGE]".into(),
                    );
                }
            }
        }
        if !valid_device_id(&result.client_id) || !valid_device_name(&result.name) {
            return Err("invalid fixture identity".into());
        }
        AssetDescriptor::from_bytes(&result.picture)?;
        Ok(result)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Arc::new(Config::parse()?);
    let listener = TcpListener::bind("0.0.0.0:0")?;
    let port = listener.local_addr()?.port();
    let mdns = ServiceDaemon::new()?;
    mdns.disable_interface(vec![IfKind::IPv6, IfKind::LoopbackV4])?;
    let properties = [
        ("v", "1"),
        ("id", config.client_id.as_str()),
        ("name", config.name.as_str()),
    ];
    let hostname = format!("loki-fixture-{}.local.", config.client_id);
    let info = ServiceInfo::new(
        "_loki-host._tcp.local.",
        &config.name,
        &hostname,
        "",
        port,
        &properties[..],
    )?
    .enable_addr_auto();
    mdns.register(info)?;
    println!(
        "FIXTURE_READY port={port} clientId={} name={} assetBytes={}; test data only.",
        config.client_id,
        config.name,
        config.picture.len()
    );
    let records = Arc::new(Mutex::new(BTreeMap::new()));
    let input = Arc::new(Mutex::new(()));
    let connections = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        struct Permit(Arc<AtomicUsize>);
        impl Drop for Permit {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        if connections.fetch_add(1, Ordering::SeqCst) >= 72 {
            connections.fetch_sub(1, Ordering::SeqCst);
            drop(stream);
            continue;
        }
        let permit = Permit(connections.clone());
        let r = records.clone();
        let i = input.clone();
        let c = config.clone();
        thread::spawn(move || {
            let _permit = permit;
            if let Err(e) = serve(stream.unwrap(), r, i, c) {
                eprintln!("Fixture connection ended: {e}");
            }
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "protocol_host_test.rs"]
mod protocol_host_test;
