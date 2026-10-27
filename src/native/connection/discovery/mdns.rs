//! mdns-sd 浏览器（桌面与 Android）。Android 宿主需在发现运行期间持有多播锁。
use super::{super::SERVICE, Event, Found, resolved};
use mdns_sd::{ScopedIp, ServiceDaemon, ServiceEvent};
use std::{sync::mpsc::SyncSender, thread, time::Duration};

/// 网卡变化另由宿主或下一轮发现处理，不需要默认每 5 秒的地址轮询。
const IP_CHECK_SECS: u32 = 60;

pub(super) struct Browser {
    daemon: ServiceDaemon,
}
impl Browser {
    pub(super) fn start(events: SyncSender<Event>) -> Result<Self, String> {
        fn failed(_: impl std::fmt::Display) -> String {
            "局域网发现不可用，请检查本地网络权限。".into()
        }
        let daemon = ServiceDaemon::new().map_err(failed)?;
        let _ = daemon.set_ip_check_interval(IP_CHECK_SECS);
        let receiver = daemon.browse(SERVICE).map_err(failed)?;
        thread::Builder::new()
            .name("loki-lan-browse".into())
            .spawn(move || {
                // 守护进程关闭后接收端结束，线程随之退出。
                while let Ok(event) = receiver.recv() {
                    let found = match event {
                        ServiceEvent::ServiceResolved(info) => {
                            if info.ty_domain != SERVICE {
                                continue;
                            }
                            let addresses = info.get_addresses().iter().filter_map(|a| match a {
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
                    if let Some(found) = found
                        && events.send(Event::Discovery(found)).is_err()
                    {
                        return;
                    }
                }
            })
            .map_err(failed)?;
        Ok(Self { daemon })
    }
    pub(super) fn stop(self) {
        let _ = self.daemon.stop_browse(SERVICE);
        if let Ok(done) = self.daemon.shutdown() {
            let _ = done.recv_timeout(Duration::from_millis(500));
        }
    }
}
