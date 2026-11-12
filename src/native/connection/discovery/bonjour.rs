//! iOS 系统 Bonjour（dns_sd）浏览器。经 mDNSResponder 发现服务，
//! 不需要多播 entitlement；Info.plist 须声明 NSBonjourServices 与本地网络用途。
//! 所有 DNSServiceRef 只在浏览线程创建、处理和释放。
use super::{super::SERVICE, Event, Found, resolved, valid_advertisement};
use std::{
    // 有序映射与有序集合
    collections::{BTreeMap, BTreeSet},
    // C 字符串与整数类型
    ffi::{CStr, CString, c_char, c_int, c_void},
    net::{IpAddr, Ipv4Addr},
    os::fd::RawFd,
    ptr,
    sync::mpsc::SyncSender,
    // 线程
    thread,
    time::{Duration, Instant},
};

#[path = "bonjour_dns.rs"]
// dns_sd 与 libc 的 FFI 声明
mod dns;
use dns::{
    // 系统 DNS-SD 接口：浏览、取地址、处理结果
    AF_INET,
    DNSServiceBrowse,
    DNSServiceGetAddrInfo,
    DNSServiceProcessResult,
    DNSServiceRefDeallocate,
    DNSServiceRefSockFD,
    DNSServiceResolve,
    DnsError,
    DnsRef,
    FLAGS_ADD,
    POLLIN,
    PROTOCOL_IPV4,
    PollFd,
    SockAddrIn,
    // 读取 TXT 记录值
    TXTRecordGetValuePtr,
    close,
    pipe,
    poll,
    write,
};

// 浏览、解析与地址回调
#[path = "bonjour_callbacks.rs"]
mod callbacks;
use callbacks::*;

// 冷启动可能与 mDNSResponder/网络就绪形成竞态。绝不能让一次静默的
// 解析永久保留:所有者会以有限退避重建失败的浏览器。
// 冷启动时可能赶在 mDNSResponder 或网络就绪之前；不能让解析无声地一直挂着，owner 会以有界退避重建失败的浏览器
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(8);

/// 一次解析：TXT 校验通过后再查 IPv4 地址。
// 一次解析：TXT 校验通过后再查 IPv4 地址
struct Pending {
    // 回调用的共享状态指针
    state: *mut State,
    key: String,
    // TXT 中的协议版本、设备 ID 与名称
    version: Option<String>,
    id: Option<String>,
    name: Option<String>,
    port: u16,
    resolve: DnsRef,
    address: DnsRef,
    deadline: Instant,
}

// 浏览线程的状态，只在该线程内访问
struct State {
    events: SyncSender<Event>,
    browse: DnsRef,
    pending: BTreeMap<String, Box<Pending>>,
    // 本轮已完成待释放的解析
    finished: Vec<String>,
    // 本轮浏览得到的增删，处理完所有就绪描述符后再统一应用
    changes: BTreeMap<String, Option<Service>>,
    // 每个服务出现在哪些网卡上
    interfaces: BTreeMap<String, BTreeSet<u32>>,
    failure: Option<String>,
}

struct Service {
    interface: u32,
    name: CString,
    regtype: CString,
    domain: CString,
}

// 浏览器句柄：持有唤醒管道的写端
pub(super) struct Browser {
    wake: RawFd,
}
impl Browser {
    // 创建唤醒管道并启动浏览线程
    pub(super) fn start(events: SyncSender<Event>) -> Result<Self, String> {
        let mut fds = [0; 2];
        // 管道创建失败即视为发现不可用
        if unsafe { pipe(fds.as_mut_ptr()) } != 0 {
            // 创建失败：提示检查本地网络权限
            return Err("局域网发现不可用，请检查本地网络权限。".into());
        }
        let (read, wake) = (fds[0], fds[1]);
        // 线程创建失败则关闭管道
        if thread::Builder::new()
            // 线程名
            .name("loki-lan-browse".into())
            // 启动后台线程
            .spawn(move || run(events, read))
            .is_err()
        {
            unsafe {
                close(read);
                close(wake);
            }
            // 浏览不可用，提示检查本地网络权限
            return Err("局域网发现不可用，请检查本地网络权限。".into());
        }
        Ok(Self { wake })
    }
    // 向管道写入一个字节唤醒浏览线程使其退出，再关闭写端
    pub(super) fn stop(self) {
        unsafe {
            // 向管道写一个字节唤醒后台线程
            write(self.wake, [1u8].as_ptr().cast(), 1);
            close(self.wake);
        }
    }
}

// 浏览线程主循环：所有 DNSServiceRef 只在这里创建、处理与释放
fn run(events: SyncSender<Event>, wake: RawFd) {
    let mut state = Box::new(State {
        events,
        browse: ptr::null_mut(),
        pending: BTreeMap::new(),
        finished: Vec::new(),
        changes: BTreeMap::new(),
        interfaces: BTreeMap::new(),
        failure: None,
    });
    // 回调通过此指针访问状态
    let context = (&mut *state as *mut State).cast::<c_void>();
    // 服务类型去掉 .local. 后缀
    let regtype = CString::new(SERVICE.trim_end_matches(".local.")).unwrap_or_default();
    // 浏览域为 local.
    let domain = CString::new("local.").unwrap_or_default();
    let status = unsafe {
        DNSServiceBrowse(
            &mut state.browse,
            0,
            0,
            regtype.as_ptr(),
            domain.as_ptr(),
            on_browse,
            context,
        )
    };
    // 启动失败：通知 owner 并退出
    if status != 0 {
        // 把失败通知给调用方
        let _ = state
            .events
            .send(Event::Discovery(Found::Failed(dns_error("Browse", status))));
        unsafe { close(wake) };
        // 结束线程
        return;
    }
    loop {
        // 收集所有需要等待的服务引用
        let mut refs = vec![state.browse];
        // 收集所有待处理项的解析与地址句柄
        for p in state.pending.values() {
            // 只保留非空句柄
            refs.extend([p.resolve, p.address].into_iter().filter(|r| !r.is_null()));
        }
        // 为每个引用取描述符，再加上唤醒管道
        let mut polls: Vec<PollFd> = refs
            .iter()
            .map(|r| PollFd {
                // 取句柄对应的套接字描述符
                fd: unsafe { DNSServiceRefSockFD(*r) },
                events: POLLIN,
                // 返回事件初始为 0
                revents: 0,
            })
            // 再追加唤醒管道的描述符
            .chain([PollFd {
                fd: wake,
                events: POLLIN,
                // 返回事件初始为 0
                revents: 0,
            }])
            .collect();
        // 等到最近的解析期限，没有则无限等待
        let timeout = state
            .pending
            .values()
            .map(|p| p.deadline)
            .min()
            // 没有则无限等待，否则换算为毫秒
            .map_or(-1, |at| {
                // 距下一个期限的剩余时间
                at.saturating_duration_since(Instant::now())
                    .as_millis()
                    // 限制在 c_int 范围内
                    .min(c_int::MAX as u128) as c_int
            });
        if unsafe { poll(polls.as_mut_ptr(), polls.len() as u32, timeout) } < 0 {
            continue;
        }
        // 唤醒管道可读：退出循环
        if polls.last().is_some_and(|p| p.revents != 0) {
            break;
        }
        // 处理所有就绪的引用
        for (r, p) in refs.iter().zip(&polls) {
            if p.revents != 0 {
                let status = unsafe { DNSServiceProcessResult(*r) };
                if status != 0 {
                    state.failure = Some(dns_error("ProcessResult", status));
                }
            }
        }
        for key in std::mem::take(&mut state.finished) {
            // 移除已完成的待处理项
            if let Some(p) = state.pending.remove(&key) {
                release(&p);
            }
        }
        // 一个 ProcessResult 批次可能对同一个实例包含 Remove 然后 Add。
        // 只有在每个轮询到的 ref 都用过之后才应用其最终状态;
        // 这样既保留了 Add,也绝不会释放仍在 `refs` 中的 ref。
        // 一批结果可能含同一实例的先删后增；等所有就绪引用处理完才应用最终状态，既保留新增，也不会释放仍在 refs 里的引用
        for (key, service) in std::mem::take(&mut state.changes) {
            // 移除对应的待处理项
            if let Some(pending) = state.pending.remove(&key) {
                release(&pending);
            }
            if let Some(service) = service {
                // 新增或更新：开始解析
                begin_resolve(&mut state, key, service);
            // 否则
            } else {
                // 删除：通知 owner
                let _ = state.events.send(Event::Discovery(Found::Removed(key)));
            }
        }
        state.expire(Instant::now());
        // 有终态失败则通知 owner 并退出
        if let Some(message) = state.failure.take() {
            // 把失败通知给调用方
            let _ = state.events.send(Event::Discovery(Found::Failed(message)));
            break;
        }
    }
    // 退出前释放所有未完成的解析与浏览请求
    for p in std::mem::take(&mut state.pending).into_values() {
        release(&p);
    }
    unsafe {
        DNSServiceRefDeallocate(state.browse);
        close(wake);
    }
}

impl State {
    // 任一解析超过期限就把整个浏览器记为失败
    fn expire(&mut self, now: Instant) {
        // 有待处理项已超过期限
        if self.pending.values().any(|pending| pending.deadline <= now) {
            self.failure
                // 记录解析超时的失败原因
                .get_or_insert_with(|| "Bonjour 解析超时，正在重新发现桌面端。".into());
        }
    }
}

// 错误文案只含阶段与状态码，不含 TXT 数据或凭据
fn dns_error(operation: &str, status: DnsError) -> String {
    // DNS 阶段/状态是公开的诊断信息,绝不是 TXT 数据或凭据。
    format!("Bonjour {operation} 失败（{status}），正在重新发现桌面端。")
}

// 释放一次解析持有的两个引用
fn release(p: &Pending) {
    // 释放解析与地址两个句柄
    for r in [p.resolve, p.address] {
        if !r.is_null() {
            unsafe { DNSServiceRefDeallocate(r) };
        }
    }
}

#[cfg(test)]
#[path = "bonjour_callback_test.rs"]
mod callback_test;
