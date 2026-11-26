//! 局域网发现：广告只提供候选端点，不代表信任。
//! Apple 移动端经系统 Bonjour（无需多播权限），其余平台用 mdns-sd；结果统一送回 owner 的事件队列。
// iOS 用系统 Bonjour，其余平台用 mdns-sd
#[cfg(target_os = "ios")]
#[path = "bonjour.rs"]
mod backend;
#[cfg(not(target_os = "ios"))]
#[path = "mdns.rs"]
mod backend;
// macOS 提供同一套 dns_sd，测试在此验证 iOS 后端的浏览、解析与 TXT 校验。
// macOS 有同一套 dns_sd，测试借此验证 iOS 后端的浏览、解析与 TXT 校验
#[cfg(all(test, target_os = "macos"))]
#[path = "bonjour.rs"]
mod bonjour;
#[cfg(all(test, target_os = "macos"))]
#[path = "bonjour_test.rs"]
mod bonjour_test;

use super::Event;
use loki_metris_pairing::{valid_device_id, valid_device_name};
use std::{
    net::{IpAddr, SocketAddr},
    sync::mpsc::SyncSender,
    time::{Duration, Instant},
};

/// 发现线程交给 owner 的事实。`key` 是服务实例全名。
// 发现线程交给 owner 的事实；key 是服务实例全名
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
    // 浏览器终态失败，仅系统 Bonjour 会报告
    Failed(String),
}

/// 校验协议 TXT 与地址：只接受私网或链路本地 IPv4。
// 校验协议 TXT 与地址：只接受私网或链路本地 IPv4
fn resolved(
    key: String,
    version: Option<&str>,
    id: Option<&str>,
    name: Option<&str>,
    addresses: impl IntoIterator<Item = IpAddr>,
    port: u16,
) -> Option<Found> {
    // 广告字段不合法则丢弃
    if !valid_advertisement(version, id, name, port) {
        // 无效则返回 None
        return None;
    }
    // 标识与名称缺一不可
    let (id, name) = (id?, name?);
    // 取第一个私网或链路本地 IPv4 地址
    let address = addresses.into_iter().find_map(|a| match a {
        // 只接受私有或链路本地的 IPv4 地址
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

// 版本必须为 "1"，ID 与名称合法，端口非零
fn valid_advertisement(
    version: Option<&str>,
    id: Option<&str>,
    name: Option<&str>,
    port: u16,
) -> bool {
    // 版本须为 1 且标识、名称都存在
    matches!((version, id, name), (Some("1"), Some(id), Some(name))
        // 标识与名称合法且端口非零
        if valid_device_id(id) && valid_device_name(name) && port != 0)
}

/// 当前浏览器与失败后的重试期限。
#[derive(Default)]
// 当前浏览器与失败后的重试期限
pub(super) struct Discovery {
    browser: Option<backend::Browser>,
    retry_at: Option<Instant>,
    // 连续失败次数，用于退避
    failures: u32,
    #[cfg(test)]
    // 测试用：禁用真实发现
    disabled: bool,
}
impl Discovery {
    /// 测试 owner 直接注入端点，不向真实网络发起多播。
    #[cfg(test)]
    // 测试 owner 直接注入端点，不向真实网络发起多播
    pub(super) fn disabled() -> Self {
        Self {
            disabled: true,
            ..Self::default()
        }
    }
    pub(super) fn running(&self) -> bool {
        self.browser.is_some()
    }
    // 启动浏览器；失败则记录并进入退避
    pub(super) fn start(&mut self, events: SyncSender<Event>) -> Result<(), String> {
        #[cfg(test)]
        if self.disabled {
            return Ok(());
        }
        if self.browser.is_none() {
            match backend::Browser::start(events) {
                Ok(browser) => self.browser = Some(browser),
                Err(error) => {
                    self.record_failure();
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    // 停止浏览器并清除退避
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

    pub(super) fn record_resolution(&mut self) {
        self.failures = 0;
    }

    /// 浏览器失败后丢弃实例，按 1/2/4/8 秒退避重建。
    // 浏览器失败后丢弃实例，按 1/2/4/8 秒退避重建
    pub(super) fn record_failure(&mut self) {
        // 先停止浏览
        self.stop();
        // 退避：1、2、4、8 秒封顶
        let delay = Duration::from_secs(1 << self.failures.min(3));
        self.failures = self.failures.saturating_add(1);
        self.retry_at = Some(Instant::now() + delay);
    }
}
