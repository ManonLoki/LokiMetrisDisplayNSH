use super::{Event, Found, bonjour::Browser};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

const ID: &str = "abababab-abab-4aba-8aba-abababababab";

/// 用 mdns-sd 发布一条协议广告，经系统 mDNSResponder 由 dns_sd 浏览器解析回来。
#[test]
fn system_bonjour_resolves_a_protocol_advertisement_with_its_txt_identity() {
    let daemon = ServiceDaemon::new().unwrap();
    let address = local_private_ipv4().expect("a private IPv4 interface");
    let info = ServiceInfo::new(
        "_loki-host._tcp.local.",
        "bonjour-backend-test",
        "bonjour-backend-test.local.",
        address.to_string(),
        47321,
        &[("v", "1"), ("id", ID), ("name", "Bonjour 后端测试")][..],
    )
    .unwrap();
    daemon.register(info).unwrap();
    let (tx, rx) = mpsc::sync_channel(64);
    let browser = Browser::start(tx).unwrap();
    let end = Instant::now() + Duration::from_secs(15);
    let found = loop {
        let remaining = end.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(Event::Discovery(Found::Resolved {
                id, name, address, ..
            })) if id == ID => break (name, address),
            Ok(_) => continue,
            Err(_) => panic!("dns_sd did not resolve the advertisement"),
        }
    };
    assert_eq!(found.0, "Bonjour 后端测试");
    assert_eq!(found.1.port(), 47321);
    browser.stop();
    let _ = daemon.shutdown();
}

fn local_private_ipv4() -> Option<std::net::Ipv4Addr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("10.255.255.255:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(v) if v.is_private() || v.is_link_local() => Some(v),
        _ => None,
    }
}
