//! iOS 系统 Bonjour（dns_sd）浏览器。经 mDNSResponder 发现服务，
//! 不需要多播 entitlement；Info.plist 须声明 NSBonjourServices 与本地网络用途。
//! 所有 DNSServiceRef 只在浏览线程创建、处理和释放。
// 引入服务类型、事件、发现结果与校验函数
use super::{super::SERVICE, Event, Found, resolved, valid_advertisement};
// 引入集合、FFI、网络、文件描述符、通道、线程与时间
use std::{
    // 有序映射与有序集合
    collections::{BTreeMap, BTreeSet},
    // C 字符串与整数类型
    ffi::{CStr, CString, c_char, c_int, c_void},
    // IP 地址类型
    net::{IpAddr, Ipv4Addr},
    // 原始文件描述符
    os::fd::RawFd,
    // 指针工具
    ptr,
    // 同步发送端
    sync::mpsc::SyncSender,
    // 线程
    thread,
    // 时长与时刻
    time::{Duration, Instant},
};

#[path = "bonjour_dns.rs"]
// dns_sd 与 libc 的 FFI 声明
mod dns;
// 引入所需的 FFI 项
use dns::{
    // 系统 DNS-SD 接口：浏览、取地址、处理结果
    AF_INET,
    // 浏览服务
    DNSServiceBrowse,
    // 查询地址
    DNSServiceGetAddrInfo,
    // 处理结果
    DNSServiceProcessResult,
    // 释放句柄
    DNSServiceRefDeallocate,
    // 取套接字描述符
    DNSServiceRefSockFD,
    // 解析服务
    DNSServiceResolve,
    // 错误码类型
    DnsError,
    // 服务句柄类型
    DnsRef,
    // 新增标志
    FLAGS_ADD,
    // 可读事件
    POLLIN,
    // IPv4 协议
    PROTOCOL_IPV4,
    // 轮询描述符
    PollFd,
    // 套接字地址
    SockAddrIn,
    // 读取 TXT 记录值
    TXTRecordGetValuePtr,
    // 关闭描述符
    close,
    // 创建管道
    pipe,
    // 轮询
    poll,
    // 写入
    write,
};

// 冷启动可能与 mDNSResponder/网络就绪形成竞态。绝不能让一次静默的
// 解析永久保留:所有者会以有限退避重建失败的浏览器。
// 冷启动时可能赶在 mDNSResponder 或网络就绪之前；不能让解析无声地一直挂着，owner 会以有界退避重建失败的浏览器
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(8);

/// 一次解析：TXT 校验通过后再查 IPv4 地址。
// 一次解析：TXT 校验通过后再查 IPv4 地址
struct Pending {
    // 回调用的共享状态指针
    state: *mut State,
    // 服务实例全名
    key: String,
    // TXT 中的协议版本、设备 ID 与名称
    version: Option<String>,
    // 设备标识
    id: Option<String>,
    // 设备名称
    name: Option<String>,
    // 端口
    port: u16,
    // 解析请求
    resolve: DnsRef,
    // 地址查询
    address: DnsRef,
    // 解析期限
    deadline: Instant,
}

// 浏览线程的状态，只在该线程内访问
struct State {
    // 事件队列
    events: SyncSender<Event>,
    // 浏览请求
    browse: DnsRef,
    // 进行中的解析
    pending: BTreeMap<String, Box<Pending>>,
    // 本轮已完成待释放的解析
    finished: Vec<String>,
    // 本轮浏览得到的增删，处理完所有就绪描述符后再统一应用
    changes: BTreeMap<String, Option<Service>>,
    // 每个服务出现在哪些网卡上
    interfaces: BTreeMap<String, BTreeSet<u32>>,
    // 本轮的终态失败
    failure: Option<String>,
}

// 待解析的服务实例
struct Service {
    // 网卡编号
    interface: u32,
    // 服务名
    name: CString,
    // 服务类型
    regtype: CString,
    // 域
    domain: CString,
}

// 浏览器句柄：持有唤醒管道的写端
pub(super) struct Browser {
    // 唤醒管道的写端
    wake: RawFd,
}
// 浏览器的方法
impl Browser {
    // 创建唤醒管道并启动浏览线程
    pub(super) fn start(events: SyncSender<Event>) -> Result<Self, String> {
        // 创建唤醒用的管道
        let mut fds = [0; 2];
        // 管道创建失败即视为发现不可用
        if unsafe { pipe(fds.as_mut_ptr()) } != 0 {
            // 创建失败：提示检查本地网络权限
            return Err("局域网发现不可用，请检查本地网络权限。".into());
        }
        // 读端与写端
        let (read, wake) = (fds[0], fds[1]);
        // 线程创建失败则关闭管道
        if thread::Builder::new()
            // 线程名
            .name("loki-lan-browse".into())
            // 启动后台线程
            .spawn(move || run(events, read))
            // 启动失败
            .is_err()
        {
            // 调用系统接口
            unsafe {
                // 关闭读端
                close(read);
                // 关闭写端
                close(wake);
            }
            // 浏览不可用，提示检查本地网络权限
            return Err("局域网发现不可用，请检查本地网络权限。".into());
        }
        // 返回唤醒句柄
        Ok(Self { wake })
    }
    // 向管道写入一个字节唤醒浏览线程使其退出，再关闭写端
    pub(super) fn stop(self) {
        // 调用系统接口
        unsafe {
            // 向管道写一个字节唤醒后台线程
            write(self.wake, [1u8].as_ptr().cast(), 1);
            // 关闭写端
            close(self.wake);
        }
    }
}

// 浏览线程主循环：所有 DNSServiceRef 只在这里创建、处理与释放
fn run(events: SyncSender<Event>, wake: RawFd) {
    // 构造堆上的状态
    let mut state = Box::new(State {
        // 事件发送端
        events,
        // 尚未浏览
        browse: ptr::null_mut(),
        // 待处理项为空
        pending: BTreeMap::new(),
        // 已完成项为空
        finished: Vec::new(),
        // 变更为空
        changes: BTreeMap::new(),
        // 网卡记录为空
        interfaces: BTreeMap::new(),
        // 尚无失败
        failure: None,
    });
    // 回调通过此指针访问状态
    let context = (&mut *state as *mut State).cast::<c_void>();
    // 服务类型去掉 .local. 后缀
    let regtype = CString::new(SERVICE.trim_end_matches(".local.")).unwrap_or_default();
    // 浏览域为 local.
    let domain = CString::new("local.").unwrap_or_default();
    // 开始浏览
    let status = unsafe {
        // 发起服务浏览
        DNSServiceBrowse(
            // 浏览句柄输出
            &mut state.browse,
            // 标志
            0,
            // 网卡
            0,
            // 服务类型
            regtype.as_ptr(),
            // 域
            domain.as_ptr(),
            // 浏览回调
            on_browse,
            // 回调上下文
            context,
        )
    };
    // 启动失败：通知 owner 并退出
    if status != 0 {
        // 把失败通知给调用方
        let _ = state
            // 取事件通道
            .events
            // 发送浏览失败事件
            .send(Event::Discovery(Found::Failed(dns_error("Browse", status))));
        // 关闭写端
        unsafe { close(wake) };
        // 结束线程
        return;
    }
    // 主循环
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
            // 遍历句柄
            .iter()
            // 转为轮询描述符
            .map(|r| PollFd {
                // 取句柄对应的套接字描述符
                fd: unsafe { DNSServiceRefSockFD(*r) },
                // 关注可读事件
                events: POLLIN,
                // 返回事件初始为 0
                revents: 0,
            })
            // 再追加唤醒管道的描述符
            .chain([PollFd {
                // 唤醒管道的描述符
                fd: wake,
                // 关注可读事件
                events: POLLIN,
                // 返回事件初始为 0
                revents: 0,
            }])
            // 收集为列表
            .collect();
        // 等到最近的解析期限，没有则无限等待
        let timeout = state
            // 待处理项
            .pending
            // 各项的值
            .values()
            // 取期限
            .map(|p| p.deadline)
            // 最早的一个
            .min()
            // 没有则无限等待，否则换算为毫秒
            .map_or(-1, |at| {
                // 距下一个期限的剩余时间
                at.saturating_duration_since(Instant::now())
                    // 转为毫秒数
                    .as_millis()
                    // 限制在 c_int 范围内
                    .min(c_int::MAX as u128) as c_int
            });
        // 等待事件，失败则
        if unsafe { poll(polls.as_mut_ptr(), polls.len() as u32, timeout) } < 0 {
            // 继续下一轮
            continue;
        }
        // 唤醒管道可读：退出循环
        if polls.last().is_some_and(|p| p.revents != 0) {
            // 结束循环
            break;
        }
        // 处理所有就绪的引用
        for (r, p) in refs.iter().zip(&polls) {
            // 该描述符有事件
            if p.revents != 0 {
                // 处理结果
                let status = unsafe { DNSServiceProcessResult(*r) };
                // 处理失败
                if status != 0 {
                    // 记录失败原因
                    state.failure = Some(dns_error("ProcessResult", status));
                }
            }
        }
        // 释放已完成的解析
        for key in std::mem::take(&mut state.finished) {
            // 移除已完成的待处理项
            if let Some(p) = state.pending.remove(&key) {
                // 释放其句柄
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
                // 释放其句柄
                release(&pending);
            }
            // 有新服务需要解析
            if let Some(service) = service {
                // 新增或更新：开始解析
                begin_resolve(&mut state, key, service);
            // 否则
            } else {
                // 删除：通知 owner
                let _ = state.events.send(Event::Discovery(Found::Removed(key)));
            }
        }
        // 检查解析是否超时
        state.expire(Instant::now());
        // 有终态失败则通知 owner 并退出
        if let Some(message) = state.failure.take() {
            // 把失败通知给调用方
            let _ = state.events.send(Event::Discovery(Found::Failed(message)));
            // 结束循环
            break;
        }
    }
    // 退出前释放所有未完成的解析与浏览请求
    for p in std::mem::take(&mut state.pending).into_values() {
        // 释放剩余待处理项
        release(&p);
    }
    // 调用系统接口
    unsafe {
        // 释放浏览句柄
        DNSServiceRefDeallocate(state.browse);
        // 关闭写端
        close(wake);
    }
}

// 状态的方法
impl State {
    // 任一解析超过期限就把整个浏览器记为失败
    fn expire(&mut self, now: Instant) {
        // 有待处理项已超过期限
        if self.pending.values().any(|pending| pending.deadline <= now) {
            // 记录解析超时
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
        // 非空才释放
        if !r.is_null() {
            // 调用系统接口
            unsafe { DNSServiceRefDeallocate(r) };
        }
    }
}

// C 字符串转 Rust 字符串，空指针得到空串
fn text(value: *const c_char) -> String {
    // 指针为空
    if value.is_null() {
        // 返回空字符串
        return String::new();
    }
    // 按 C 字符串读取
    unsafe { CStr::from_ptr(value) }
        // 宽容地转为字符串
        .to_string_lossy()
        // 转为自有字符串
        .into_owned()
}

// 浏览回调：记录服务在各网卡上的出现与消失
unsafe extern "C" fn on_browse(
    // 句柄
    _: DnsRef,
    // 标志
    flags: u32,
    // 网卡
    interface: u32,
    // 错误码
    error: DnsError,
    // 服务名
    name: *const c_char,
    // 服务类型
    regtype: *const c_char,
    // 域
    domain: *const c_char,
    // 回调上下文
    context: *mut c_void,
    // 回调参数结束
) {
    // 还原状态引用
    let state = unsafe { &mut *context.cast::<State>() };
    // 有错误
    if error != 0 {
        // 记录失败原因
        state.failure = Some(dns_error("Browse", error));
        // 然后返回
        return;
    }
    // 服务实例全名
    let key = format!("{}.{}{}", text(name), text(regtype), text(domain));
    // 服务数与待处理变化数各有 64 的上限
    if !state.interfaces.contains_key(&key) && state.interfaces.len() >= 64 {
        // 直接返回
        return;
    }
    // 变更队列已满且是新键
    if !state.changes.contains_key(&key) && state.changes.len() >= 64 {
        // 丢弃
        return;
    }
    // 消失：从该网卡移除；所有网卡都没了才算真正移除
    if flags & FLAGS_ADD == 0 {
        // 取该服务的网卡集合
        let Some(interfaces) = state.interfaces.get_mut(&key) else {
            // 没有则返回
            return;
        };
        // 移除此网卡
        if !interfaces.remove(&interface) {
            // 原本就没有则返回
            return;
        }
        // 已没有任何网卡
        if interfaces.is_empty() {
            // 移除该网卡记录
            state.interfaces.remove(&key);
            // 记录该服务消失
            state.changes.insert(key, None);
            // 然后返回
            return;
        }
    // 出现：记录网卡，每个服务最多 16 个
    } else {
        // 取或建该服务的网卡集合
        let interfaces = state.interfaces.entry(key.clone()).or_default();
        // 网卡数达到上限且是新网卡
        if interfaces.len() >= 16 && !interfaces.contains(&interface) {
            // 丢弃
            return;
        }
        // 登记此网卡
        interfaces.insert(interface);
    }
    // 所有者键仍是完整的服务名,而不是同一台桌面的各网卡别名。
    // 如果某个网卡消失,就重新解析仍然存活的那个。
    // owner 的键始终是完整服务名，而不是每网卡的别名；某网卡消失时改用留下的那一个重新解析
    let Some(&interface) = state.interfaces[&key].first() else {
        // 直接返回
        return;
    };
    // 名称、类型与域都必须合法
    let (Ok(name), Ok(regtype), Ok(domain)) = (
        // 服务名转 C 字符串
        CString::new(text(name)),
        // 服务类型转 C 字符串
        CString::new(text(regtype)),
        // 域转 C 字符串
        CString::new(text(domain)),
        // 任一失败则放弃
    ) else {
        // 然后返回
        return;
    };
    // 记录待解析的服务
    state.changes.insert(
        // 服务键
        key,
        // 构造服务
        Some(Service {
            // 网卡
            interface,
            // 服务名
            name,
            // 服务类型
            regtype,
            // 域
            domain,
        }),
    );
}

// 开始解析服务实例
fn begin_resolve(state: &mut State, key: String, service: Service) {
    // 解析并发上限 64
    if state.pending.len() >= 64 {
        // 直接返回
        return;
    }
    // 构造待解析项
    let mut pending = Box::new(Pending {
        // 指向状态的指针
        state: state as *mut State,
        // 服务键
        key: key.clone(),
        // 尚无版本
        version: None,
        // 尚无标识
        id: None,
        // 尚无名称
        name: None,
        // 尚无端口
        port: 0,
        // 尚未发起解析
        resolve: ptr::null_mut(),
        // 尚无地址查询
        address: ptr::null_mut(),
        // 解析期限
        deadline: Instant::now() + RESOLVE_TIMEOUT,
    });
    // 回调上下文指向待解析项
    let context = (&mut *pending as *mut Pending).cast::<c_void>();
    // 发起服务解析
    let status = unsafe {
        // 调用系统接口
        DNSServiceResolve(
            // 解析句柄
            &mut pending.resolve,
            // 无标志
            0,
            // 网卡
            service.interface,
            // 服务名
            service.name.as_ptr(),
            // 服务类型
            service.regtype.as_ptr(),
            // 域
            service.domain.as_ptr(),
            // 解析回调
            on_resolve,
            // 回调上下文
            context,
        )
    };
    // 成功则登记，否则记为失败
    if status == 0 {
        // 登记待处理项
        state.pending.insert(key, pending);
    // 发起解析失败
    } else {
        // 记录失败原因
        state.failure = Some(dns_error("Resolve", status));
    }
}

// 读取 TXT 中的 UTF-8 值
fn txt_value(len: u16, txt: *const u8, key: &str) -> Option<String> {
    // 键转为 C 字符串
    let key = CString::new(key).ok()?;
    // 值长度
    let mut size = 0u8;
    // 在 TXT 记录里查找该键
    let value = unsafe { TXTRecordGetValuePtr(len, txt, key.as_ptr(), &mut size) };
    // 没有该键
    if value.is_null() {
        // 返回 None
        return None;
    }
    // 按长度取出字节
    let bytes = unsafe { std::slice::from_raw_parts(value, size as usize) };
    // 转为字符串，非法则 None
    String::from_utf8(bytes.to_vec()).ok()
}

// 解析回调：读取 TXT，校验通过再查 IPv4 地址
unsafe extern "C" fn on_resolve(
    // 句柄
    _: DnsRef,
    // 标志
    _: u32,
    // 网卡
    interface: u32,
    // 错误码
    error: DnsError,
    // 完整名称
    _: *const c_char,
    // 主机名
    host: *const c_char,
    // 端口
    port: u16,
    // TXT 记录长度
    txt_len: u16,
    // TXT 记录内容
    txt: *const u8,
    // 回调上下文
    context: *mut c_void,
    // 回调参数结束
) {
    // 还原待解析项
    let pending = unsafe { &mut *context.cast::<Pending>() };
    // 还原状态引用
    let state = unsafe { &mut *pending.state };
    // 有错误
    if error != 0 {
        // 记录失败原因
        state.failure = Some(dns_error("Resolve", error));
        // 然后返回
        return;
    }
    // 已在查地址则忽略重复回调
    if !pending.address.is_null() {
        // 直接返回
        return;
    }
    // 读取协议版本、设备 ID、名称与端口
    pending.version = txt_value(txt_len, txt, "v");
    // 读取 id
    pending.id = txt_value(txt_len, txt, "id");
    // 读取名称
    pending.name = txt_value(txt_len, txt, "name");
    // 端口为网络字节序
    pending.port = u16::from_be(port);
    // 无效的通告不得导致对其他每个服务的永久重试循环。
    // 在查询地址之前就拒绝它们的 TXT。
    // 非法的广告不能让其他服务陷入永久重试：地址查询前就拒绝其 TXT
    if !valid_advertisement(
        // 版本
        pending.version.as_deref(),
        // 标识
        pending.id.as_deref(),
        // 名称
        pending.name.as_deref(),
        // 端口
        pending.port,
        // 构造成功则继续
    ) {
        // 记录已完成的键
        state.finished.push(pending.key.clone());
        // 移除网卡记录
        state.interfaces.remove(&pending.key);
        // 然后返回
        return;
    }
    // 发起 IPv4 地址查询
    let status = unsafe {
        // 查询地址
        DNSServiceGetAddrInfo(
            // 地址句柄输出
            &mut pending.address,
            // 标志
            0,
            // 网卡
            interface,
            // IPv4 协议
            PROTOCOL_IPV4,
            // 主机名
            host,
            // 地址回调
            on_address,
            // 回调上下文
            context,
        )
    };
    // 查询地址失败
    if status != 0 {
        // 清空地址句柄
        pending.address = ptr::null_mut();
        // 记录失败原因
        state.failure = Some(dns_error("GetAddrInfo", status));
    }
}

// 地址回调：拿到可用的 IPv4 就报告解析完成
unsafe extern "C" fn on_address(
    // 句柄
    _: DnsRef,
    // 标志
    flags: u32,
    // 网卡
    _: u32,
    // 错误码
    error: DnsError,
    // 完整名称
    _: *const c_char,
    // 地址
    address: *const SockAddrIn,
    // 生存期
    _: u32,
    // 回调上下文
    context: *mut c_void,
    // 回调参数结束
) {
    // 还原待解析项
    let pending = unsafe { &mut *context.cast::<Pending>() };
    // 还原状态引用
    let state = unsafe { &mut *pending.state };
    // 有错误
    if error != 0 {
        // 记录失败原因
        state.failure = Some(dns_error("GetAddrInfo", error));
        // 然后返回
        return;
    }
    // 最初的否定/移除结果不是可用的端点。保留该查询,
    // 直到 Add 到达或有限的解析期限届满。
    // 起初的否定或删除结果不是可用端点；保留查询直到出现新增结果或解析期限届满
    if flags & FLAGS_ADD == 0 || address.is_null() {
        // 直接返回
        return;
    }
    // 还原地址引用
    let address = unsafe { &*address };
    // 只处理 IPv4
    if address.family != AF_INET {
        // 忽略其他
        return;
    }
    // 由四字节构造 IPv4 地址
    let ip = IpAddr::V4(Ipv4Addr::from(address.addr));
    // 通过校验才算解析成功
    if let Some(found) = resolved(
        // 服务键
        pending.key.clone(),
        // 版本
        pending.version.as_deref(),
        // 标识
        pending.id.as_deref(),
        // 名称
        pending.name.as_deref(),
        // 地址列表
        [ip],
        // 端口
        pending.port,
        // 构造成功则继续
    ) {
        // 标记该项已完成
        state.finished.push(pending.key.clone());
        // 发出发现事件
        let _ = state.events.send(Event::Discovery(found));
    }
}

// 测试模块
#[cfg(test)]
#[path = "bonjour_callback_test.rs"]
mod callback_test;
