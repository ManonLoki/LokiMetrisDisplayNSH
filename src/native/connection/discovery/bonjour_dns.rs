//! Darwin 的 dns_sd 与 poll ABI,仅在浏览器线程上使用。
// 引入 C 类型
use std::ffi::{c_char, c_int, c_void};

// dns_sd 服务引用
pub(super) type DnsRef = *mut c_void;
// 错误码
pub(super) type DnsError = i32;
// 回调标志：新增
pub(super) const FLAGS_ADD: u32 = 0x2;
// 只查询 IPv4
pub(super) const PROTOCOL_IPV4: u32 = 0x1;
// IPv4 地址族
pub(super) const AF_INET: u8 = 2;
// 浏览回调签名
type BrowseReply = unsafe extern "C" fn(
    // 服务句柄
    DnsRef,
    // 标志
    u32,
    // 网卡
    u32,
    // 错误码
    DnsError,
    *const c_char,
    *const c_char,
    *const c_char,
    *mut c_void,
);
// 解析回调签名
type ResolveReply = unsafe extern "C" fn(
    // 服务句柄
    DnsRef,
    // 标志
    u32,
    // 网卡
    u32,
    // 错误码
    DnsError,
    *const c_char,
    *const c_char,
    // 端口
    u16,
    // 端口类型
    u16,
    *const u8,
    *mut c_void,
);
// 地址查询回调签名
type AddrReply = unsafe extern "C" fn(
    // 服务句柄
    DnsRef,
    // 标志
    u32,
    // 网卡
    u32,
    // 错误码
    DnsError,
    *const c_char,
    *const SockAddrIn,
    // 生存期
    u32,
    *mut c_void,
);

#[repr(C)]
// Darwin 的 IPv4 套接字地址布局
pub(super) struct SockAddrIn {
    // 长度
    pub(super) len: u8,
    // 地址族
    pub(super) family: u8,
    // 端口
    pub(super) port: u16,
    // IPv4 地址
    pub(super) addr: [u8; 4],
    // 填充
    pub(super) zero: [u8; 8],
}
#[repr(C)]
// poll 的描述符结构
pub(super) struct PollFd {
    // 描述符
    pub(super) fd: c_int,
    // 关注的事件
    pub(super) events: i16,
    // 返回的事件
    pub(super) revents: i16,
}
// 可读事件
pub(super) const POLLIN: i16 = 0x1;

// 系统 dns_sd 与 libc 接口
unsafe extern "C" {
    // 浏览服务
    pub(super) fn DNSServiceBrowse(
        // 句柄输出
        r: *mut DnsRef,
        // 标志
        flags: u32,
        // 网卡
        interface: u32,
        // 服务类型
        regtype: *const c_char,
        // 域
        domain: *const c_char,
        // 回调
        reply: BrowseReply,
        // 回调上下文
        context: *mut c_void,
        // 返回错误码
    ) -> DnsError;
    // 解析服务实例
    pub(super) fn DNSServiceResolve(
        // 句柄输出
        r: *mut DnsRef,
        // 标志
        flags: u32,
        // 网卡
        interface: u32,
        // 服务名
        name: *const c_char,
        // 服务类型
        regtype: *const c_char,
        // 域
        domain: *const c_char,
        // 回调
        reply: ResolveReply,
        // 回调上下文
        context: *mut c_void,
        // 返回错误码
    ) -> DnsError;
    // 查询主机地址
    pub(super) fn DNSServiceGetAddrInfo(
        // 句柄输出
        r: *mut DnsRef,
        // 标志
        flags: u32,
        // 网卡
        interface: u32,
        // 协议
        protocol: u32,
        // 主机名
        host: *const c_char,
        // 回调
        reply: AddrReply,
        // 回调上下文
        context: *mut c_void,
        // 返回错误码
    ) -> DnsError;
    // 取得服务引用对应的文件描述符
    pub(super) fn DNSServiceRefSockFD(r: DnsRef) -> c_int;
    // 处理待决结果并触发回调
    pub(super) fn DNSServiceProcessResult(r: DnsRef) -> DnsError;
    // 释放服务引用
    pub(super) fn DNSServiceRefDeallocate(r: DnsRef);
    // 读取 TXT 记录中的值
    pub(super) fn TXTRecordGetValuePtr(
        // 记录长度
        len: u16,
        // 记录内容
        txt: *const u8,
        // 键
        key: *const c_char,
        // 值长度输出
        value_len: *mut u8,
        // 返回值指针
    ) -> *const u8;
    // 创建管道，用于唤醒浏览线程
    pub(super) fn pipe(fds: *mut c_int) -> c_int;
    // 写管道
    pub(super) fn write(fd: c_int, buf: *const c_void, n: usize) -> isize;
    // 关闭描述符
    pub(super) fn close(fd: c_int) -> c_int;
    // 等待描述符可读
    pub(super) fn poll(fds: *mut PollFd, n: u32, timeout: c_int) -> c_int;
}
