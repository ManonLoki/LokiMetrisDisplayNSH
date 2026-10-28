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

// 引入端点、事件与 owner
use super::{Endpoint, Event, owner::Owner};
// 引入设备 ID 与名称校验
use loki_metris_pairing::{valid_device_id, valid_device_name};
// 引入网络、通道与时间
use std::{
    // 网络地址类型
    net::{IpAddr, SocketAddr},
    // 同步发送端
    sync::mpsc::SyncSender,
    // 时长与时刻
    time::{Duration, Instant},
};

/// 发现线程交给 owner 的事实。`key` 是服务实例全名。
// 发现线程交给 owner 的事实；key 是服务实例全名
pub(super) enum Found {
    // 已解析出端点
    Resolved {
        // 服务键
        key: String,
        // 标识
        id: String,
        // 名称
        name: String,
        // 地址
        address: SocketAddr,
    },
    // 服务消失
    Removed(String),
    /// 只有系统 Bonjour 会报告浏览器终态失败。
    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    // 浏览器终态失败，仅系统 Bonjour 会报告
    Failed(String),
}

/// 校验协议 TXT 与地址：只接受私网或链路本地 IPv4。
// 校验协议 TXT 与地址：只接受私网或链路本地 IPv4
fn resolved(
    // 服务键
    key: String,
    // 版本
    version: Option<&str>,
    // 标识
    id: Option<&str>,
    // 名称
    name: Option<&str>,
    // 地址列表
    addresses: impl IntoIterator<Item = IpAddr>,
    // 端口
    port: u16,
    // 返回发现结果
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
        // 其余地址忽略
        _ => None,
        // 取第一个可用地址
    })?;
    // 构造已解析结果
    Some(Found::Resolved {
        // 服务键
        key,
        // 标识
        id: id.into(),
        // 名称
        name: name.into(),
        // 地址
        address,
    })
}

// 版本必须为 "1"，ID 与名称合法，端口非零
fn valid_advertisement(
    // 版本
    version: Option<&str>,
    // 标识
    id: Option<&str>,
    // 名称
    name: Option<&str>,
    // 端口
    port: u16,
    // 返回是否有效
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
    // 浏览器
    browser: Option<backend::Browser>,
    // 下次重试时刻
    retry_at: Option<Instant>,
    // 连续失败次数，用于退避
    failures: u32,
    #[cfg(test)]
    // 测试用：禁用真实发现
    disabled: bool,
}
// 发现状态的方法
impl Discovery {
    /// 测试 owner 直接注入端点，不向真实网络发起多播。
    #[cfg(test)]
    // 测试 owner 直接注入端点，不向真实网络发起多播
    pub(super) fn disabled() -> Self {
        // 构造发现状态
        Self {
            // 初始为停用
            disabled: true,
            // 其余字段取默认值
            ..Self::default()
        }
    }
    // 浏览器是否在运行
    pub(super) fn running(&self) -> bool {
        // 是否在运行
        self.browser.is_some()
    }
    // 启动浏览器；失败则记录并进入退避
    pub(super) fn start(&mut self, events: SyncSender<Event>) -> Result<(), String> {
        #[cfg(test)]
        if self.disabled {
            // 已停用则直接成功
            return Ok(());
        }
        // 尚未启动浏览
        if self.browser.is_none() {
            // 启动浏览
            match backend::Browser::start(events) {
                // 成功则保存
                Ok(browser) => self.browser = Some(browser),
                // 浏览启动失败
                Err(error) => {
                    // 标记失败
                    self.failed();
                    // 返回错误
                    return Err(error);
                }
            }
        }
        // 完成
        Ok(())
    }
    // 停止浏览器并清除退避
    pub(super) fn stop(&mut self) {
        // 取走浏览器
        if let Some(browser) = self.browser.take() {
            // 停止浏览
            browser.stop();
        }
        // 清除重试时刻
        self.retry_at = None;
    }
    // 下次重试时间
    pub(super) fn retry_at(&self) -> Option<Instant> {
        // 下次重试时刻
        self.retry_at
    }
    // 清除退避
    pub(super) fn clear_retry(&mut self) {
        // 清除重试时刻
        self.retry_at = None;
    }
    /// 浏览器失败后丢弃实例，按 1/2/4/8 秒退避重建。
    // 浏览器失败后丢弃实例，按 1/2/4/8 秒退避重建
    fn failed(&mut self) {
        // 先停止浏览
        self.stop();
        // 退避：1、2、4、8 秒封顶
        let delay = Duration::from_secs(1 << self.failures.min(3));
        // 失败次数加一
        self.failures = self.failures.saturating_add(1);
        // 设定重试时刻
        self.retry_at = Some(Instant::now() + delay);
    }
}

// 所有者的方法
impl Owner {
    // owner 处理发现事件
    pub(super) fn found(&mut self, found: Found) {
        // 按发现结果处理
        match found {
            // 已解析
            Found::Resolved {
                // 服务键
                key,
                // 标识
                id,
                // 名称
                name,
                // 地址
                address,
                // 分支开始
            } => {
                // 忽略自己，端点数量上限 64
                if id == self.id
                    // 或者端点已满且是新键
                    || (self.endpoints.len() >= 64 && !self.endpoints.contains_key(&key))
                {
                    // 直接返回
                    return;
                }
                // 成功解析后清零失败计数
                self.discovery.failures = 0;
                // 同一服务的地址是否变化
                let moved = self
                    // 已有端点
                    .endpoints
                    // 按键取
                    .get(&key)
                    // 地址不同
                    .is_some_and(|e| e.address != address);
                // 记录端点
                self.endpoints.insert(
                    // 键
                    key,
                    // 端点
                    Endpoint {
                        // 标识
                        id: id.clone(),
                        // 名称
                        name,
                        // 地址
                        address,
                    },
                );
                // 是否为当前选中的桌面
                let selected = self.selected.as_deref() == Some(id.as_str());
                // 选中的桌面地址变了：当前连接失效，重新恢复
                if moved && selected && self.socket.as_ref().is_some_and(|w| w.peer != address) {
                    // 地址变了：记失败并等待恢复
                    self.failed("桌面地址已变化，正在恢复连接…".into());
                // 还没选择而发现了已配对的桌面：自动选择并连接
                } else if self.selected.is_none() && self.records.contains_key(&id) {
                    // 记录选中项
                    self.selected = Some(id.clone());
                    // 更新视图里的选中项
                    self.view["selectedId"] = serde_json::json!(id);
                    // 立即安排重试
                    self.retry_at = Some(Instant::now());
                // 选中的桌面重新出现且当前没有连接：立即重试
                } else if self.socket.is_none() && selected && self.active() {
                    // 重试计数归零
                    self.retries = 0;
                    // 立即安排重试
                    self.retry_at = Some(Instant::now());
                }
            }
            // 服务消失
            Found::Removed(key) => {
                // 移除端点
                self.endpoints.remove(&key);
            }
            // 发现失败
            Found::Failed(message) => {
                // 把错误写进视图
                self.view["error"] = serde_json::json!(message);
                // 标记失败
                self.discovery.failed();
            }
        }
        // 发布视图
        self.publish();
    }
}
