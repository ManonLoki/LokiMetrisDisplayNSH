//! mdns-sd 浏览器（桌面与 Android）。Android 宿主需在发现运行期间持有多播锁。
// 引入服务类型、事件与解析函数
use super::{super::SERVICE, Event, Found, resolved};
// 引入 mdns-sd
use mdns_sd::{ScopedIp, ServiceDaemon, ServiceEvent};
// 引入通道、线程与时间
use std::{sync::mpsc::SyncSender, thread, time::Duration};

/// 网卡变化另由宿主或下一轮发现处理，不需要默认每 5 秒的地址轮询。
// 网卡变化另由宿主或下一轮发现处理，不需要默认每 5 秒轮询地址
const IP_CHECK_SECS: u32 = 60;

// mdns-sd 浏览器（桌面与 Android）；Android 宿主需在发现期间持有多播锁
pub(super) struct Browser {
    // 守护进程
    daemon: ServiceDaemon,
}
// 浏览器的方法
impl Browser {
    // 启动浏览器和接收线程
    pub(super) fn start(events: SyncSender<Event>) -> Result<Self, String> {
        // 统一的失败文案
        fn failed(_: impl std::fmt::Display) -> String {
            // 局域网发现不可用
            "局域网发现不可用，请检查本地网络权限。".into()
        }
        // 创建守护进程
        let daemon = ServiceDaemon::new().map_err(failed)?;
        // 设置地址检查间隔
        let _ = daemon.set_ip_check_interval(IP_CHECK_SECS);
        // 开始浏览目标服务
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
                    // 按事件处理
                    let found = match event {
                        // 已解析：只取 IPv4 地址并校验广告
                        ServiceEvent::ServiceResolved(info) => {
                            // 服务类型不符
                            if info.ty_domain != SERVICE {
                                // 跳过
                                continue;
                            }
                            // 收集地址
                            let addresses = info.get_addresses().iter().filter_map(|a| match a {
                                // 只取 IPv4
                                ScopedIp::V4(v) => Some((*v.addr()).into()),
                                // 其余事件忽略
                                _ => None,
                            });
                            // 构造解析结果
                            resolved(
                                // 完整名称
                                info.get_fullname().into(),
                                // 版本
                                info.get_property_val_str("v"),
                                // 标识
                                info.get_property_val_str("id"),
                                // 名称
                                info.get_property_val_str("name"),
                                // 地址列表
                                addresses.collect::<Vec<_>>(),
                                // 端口
                                info.get_port(),
                            )
                        }
                        // 服务消失
                        ServiceEvent::ServiceRemoved(_, fullname) => Some(Found::Removed(fullname)),
                        // 其余事件忽略
                        _ => None,
                    };
                    // 把结果送回 owner；队列已关闭则退出线程
                    if let Some(found) = found
                        // 并且发送发现事件失败
                        && events.send(Event::Discovery(found)).is_err()
                    {
                        // 返回
                        return;
                    }
                }
            })
            // 失败转为错误
            .map_err(failed)?;
        // 返回守护进程句柄
        Ok(Self { daemon })
    }
    // 停止浏览并关闭守护进程，最多等待 500 毫秒
    pub(super) fn stop(self) {
        // 停止浏览
        let _ = self.daemon.stop_browse(SERVICE);
        // 关闭守护进程
        if let Ok(done) = self.daemon.shutdown() {
            // 最多等待 500 毫秒
            let _ = done.recv_timeout(Duration::from_millis(500));
        }
    }
}
