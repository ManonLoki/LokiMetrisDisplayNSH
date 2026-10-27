//! 局域网发现：广告只提供候选端点，不代表信任。
//! Apple 移动端经系统 Bonjour（无需多播权限），其余平台用 mdns-sd；结果统一送回 owner 的事件队列。
#[cfg(target_os = "ios")]
#[path = "bonjour.rs"]
mod backend;
#[cfg(not(target_os = "ios"))]
#[path = "mdns.rs"]
mod backend;
// macOS 提供同一套 dns_sd，测试在此验证 iOS 后端的浏览、解析与 TXT 校验。
#[cfg(all(test, target_os = "macos"))]
#[path = "bonjour.rs"]
mod bonjour;
#[cfg(all(test, target_os = "macos"))]
#[path = "bonjour_test.rs"]
mod bonjour_test;

use super::{Endpoint, Event, owner::Owner};
use loki_metris_pairing::{valid_device_id, valid_device_name};
use std::{
    net::{IpAddr, SocketAddr},
    sync::mpsc::SyncSender,
    time::{Duration, Instant},
};

/// 发现线程交给 owner 的事实。`key` 是服务实例全名。
pub(super) enum Found {
    Resolved {
        key: String,
        id: String,
        name: String,
        address: SocketAddr,
    },
    Removed(String),
    /// 只有系统 Bonjour 会报告浏览器终态失败。
    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    Failed(String),
}

/// 校验协议 TXT 与地址：只接受私网或链路本地 IPv4。
fn resolved(
    key: String,
    version: Option<&str>,
    id: Option<&str>,
    name: Option<&str>,
    addresses: impl IntoIterator<Item = IpAddr>,
    port: u16,
) -> Option<Found> {
    if !valid_advertisement(version, id, name, port) {
        return None;
    }
    let (id, name) = (id?, name?);
    let address = addresses.into_iter().find_map(|a| match a {
        IpAddr::V4(v) if v.is_private() || v.is_link_local() => Some(SocketAddr::new(a, port)),
        _ => None,
    })?;
    Some(Found::Resolved {
        key,
        id: id.into(),
        name: name.into(),
        address,
    })
}

fn valid_advertisement(
    version: Option<&str>,
    id: Option<&str>,
    name: Option<&str>,
    port: u16,
) -> bool {
    matches!((version, id, name), (Some("1"), Some(id), Some(name))
        if valid_device_id(id) && valid_device_name(name) && port != 0)
}

/// 当前浏览器与失败后的重试期限。
#[derive(Default)]
pub(super) struct Discovery {
    browser: Option<backend::Browser>,
    retry_at: Option<Instant>,
    failures: u32,
    #[cfg(test)]
    disabled: bool,
}
impl Discovery {
    /// 测试 owner 直接注入端点，不向真实网络发起多播。
    #[cfg(test)]
    pub(super) fn disabled() -> Self {
        Self {
            disabled: true,
            ..Self::default()
        }
    }
    pub(super) fn running(&self) -> bool {
        self.browser.is_some()
    }
    pub(super) fn start(&mut self, events: SyncSender<Event>) -> Result<(), String> {
        #[cfg(test)]
        if self.disabled {
            return Ok(());
        }
        if self.browser.is_none() {
            match backend::Browser::start(events) {
                Ok(browser) => self.browser = Some(browser),
                Err(error) => {
                    self.failed();
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    pub(super) fn stop(&mut self) {
        if let Some(browser) = self.browser.take() {
            browser.stop();
        }
        self.retry_at = None;
    }
    pub(super) fn retry_at(&self) -> Option<Instant> {
        self.retry_at
    }
    pub(super) fn clear_retry(&mut self) {
        self.retry_at = None;
    }
    /// 浏览器失败后丢弃实例，按 1/2/4/8 秒退避重建。
    fn failed(&mut self) {
        self.stop();
        let delay = Duration::from_secs(1 << self.failures.min(3));
        self.failures = self.failures.saturating_add(1);
        self.retry_at = Some(Instant::now() + delay);
    }
}

impl Owner {
    pub(super) fn found(&mut self, found: Found) {
        match found {
            Found::Resolved {
                key,
                id,
                name,
                address,
            } => {
                if id == self.id
                    || (self.endpoints.len() >= 64 && !self.endpoints.contains_key(&key))
                {
                    return;
                }
                self.discovery.failures = 0;
                let moved = self
                    .endpoints
                    .get(&key)
                    .is_some_and(|e| e.address != address);
                self.endpoints.insert(
                    key,
                    Endpoint {
                        id: id.clone(),
                        name,
                        address,
                    },
                );
                let selected = self.selected.as_deref() == Some(id.as_str());
                if moved && selected && self.socket.as_ref().is_some_and(|w| w.peer != address) {
                    self.failed("桌面地址已变化，正在恢复连接…".into());
                } else if self.selected.is_none() && self.records.contains_key(&id) {
                    self.selected = Some(id.clone());
                    self.view["selectedId"] = serde_json::json!(id);
                    self.retry_at = Some(Instant::now());
                } else if self.socket.is_none() && selected && self.active() {
                    self.retries = 0;
                    self.retry_at = Some(Instant::now());
                }
            }
            Found::Removed(key) => {
                self.endpoints.remove(&key);
            }
            Found::Failed(message) => {
                self.view["error"] = serde_json::json!(message);
                self.discovery.failed();
            }
        }
        self.publish();
    }
}
