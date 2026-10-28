// 引入被测浏览器
use super::{Event, Found, bonjour::Browser};
// 用 mdns-sd 发布一个测试广告
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

// 测试用设备 ID
const ID: &str = "abababab-abab-4aba-8aba-abababababab";

/// 用 mdns-sd 发布一条协议广告，经系统 mDNSResponder 由 dns_sd 浏览器解析回来。
#[test]
// 系统 Bonjour 能解析协议广告并读出 TXT 中的身份
fn system_bonjour_resolves_a_protocol_advertisement_with_its_txt_identity() {
    let daemon = ServiceDaemon::new().unwrap();
    let address = local_private_ipv4().expect("a private IPv4 interface");
    let info = ServiceInfo::new(
        "_loki-host._tcp.local.",
        "bonjour-backend-test",
        "bonjour-backend-test.local.",
        address.to_string(),
        // 端口
        47321,
        // TXT 属性：版本、标识与名称
        &[("v", "1"), ("id", ID), ("name", "Bonjour 后端测试")][..],
    )
    // 构造成功
    .unwrap();
    // 注册服务
    daemon.register(info).unwrap();
    // 事件通道
    let (tx, rx) = mpsc::sync_channel(64);
    // 启动浏览
    let browser = Browser::start(tx).unwrap();
    // 最长等待 15 秒
    let end = Instant::now() + Duration::from_secs(15);
    // 循环等待发现事件
    let found = loop {
        // 剩余等待时间
        let remaining = end.saturating_duration_since(Instant::now());
        // 等待下一条事件
        match rx.recv_timeout(remaining) {
            // 已解析的发现
            Ok(Event::Discovery(Found::Resolved {
                // 标识、名称与地址
                id, name, address, ..
            // 标识匹配则得到结果
            })) if id == ID => break (name, address),
            // 其他事件继续等
            Ok(_) => continue,
            // 超时则测试失败
            Err(_) => panic!("dns_sd did not resolve the advertisement"),
        }
    };
    // 名称一致
    assert_eq!(found.0, "Bonjour 后端测试");
    // 端口一致
    assert_eq!(found.1.port(), 47321);
    // 停止浏览
    browser.stop();
    // 关闭守护进程
    let _ = daemon.shutdown();
}

// 取本机的私网 IPv4 地址
fn local_private_ipv4() -> Option<std::net::Ipv4Addr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("10.255.255.255:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(v) if v.is_private() || v.is_link_local() => Some(v),
        _ => None,
    }
}
