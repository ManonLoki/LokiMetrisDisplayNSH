//! mdns-sd 浏览器（桌面与 Android）。Android 宿主需在发现运行期间持有多播锁。
use super::{super::SERVICE, Event, Found, resolved};
use mdns_sd::{ScopedIp, ServiceDaemon, ServiceEvent};
use std::{sync::mpsc::SyncSender, thread, time::Duration};

/// 网卡变化另由宿主或下一轮发现处理，不需要默认每 5 秒的地址轮询。
// 网卡变化另由宿主或下一轮发现处理，不需要默认每 5 秒轮询地址
const IP_CHECK_SECS: u32 = 60;

// mdns-sd 浏览器（桌面与 Android）；Android 宿主需在发现期间持有多播锁
pub(super) struct Browser {
    daemon: ServiceDaemon,
}
impl Browser {
    // 启动浏览器和接收线程
    pub(super) fn start(events: SyncSender<Event>) -> Result<Self, String> {
        fn failed(_: impl std::fmt::Display) -> String {
            "局域网发现不可用，请检查本地网络权限。".into()
        }
        let daemon = ServiceDaemon::new().map_err(failed)?;
        let _ = daemon.set_ip_check_interval(IP_CHECK_SECS);
        let receiver = daemon.browse(SERVICE).map_err(failed)?;
        // 启动接收线程
        thread::Builder::new()
            // 线程名
            .name("loki-lan-browse".into())
            // 启动线程
            .spawn(move || {
                // 守护进程关闭后接收端结束，线程随之退出。
                // 守护进程关闭后接收端结束，线程随之退出
                while let Ok(event) = receiver.recv() {
                    let found = match event {
                        // 已解析：只取 IPv4 地址并校验广告
                        ServiceEvent::ServiceResolved(info) => {
                            if info.ty_domain != SERVICE {
                                continue;
                            }
                            let addresses = info.get_addresses().iter().filter_map(|a| match a {
                                // 只取 IPv4
                                ScopedIp::V4(v) => Some((*v.addr()).into()),
                                _ => None,
                            });
                            resolved(
                                info.get_fullname().into(),
                                info.get_property_val_str("v"),
                                info.get_property_val_str("id"),
                                info.get_property_val_str("name"),
                                addresses.collect::<Vec<_>>(),
                                info.get_port(),
                            )
                        }
                        ServiceEvent::ServiceRemoved(_, fullname) => Some(Found::Removed(fullname)),
                        _ => None,
                    };
                    // 把结果送回 owner；队列已关闭则退出线程
                    if let Some(found) = found
                        // 并且发送发现事件失败
                        && events.send(Event::Discovery(found)).is_err()
                    {
                        return;
                    }
                }
            })
            .map_err(failed)?;
        Ok(Self { daemon })
    }
    // 停止浏览并关闭守护进程，最多等待 500 毫秒
    pub(super) fn stop(self) {
        let _ = self.daemon.stop_browse(SERVICE);
        if let Ok(done) = self.daemon.shutdown() {
            // 最多等待 500 毫秒
            let _ = done.recv_timeout(Duration::from_millis(500));
        }
    }
}
