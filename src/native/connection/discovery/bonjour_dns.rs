//! Darwin dns_sd and poll ABI used only on the browser thread.
use std::ffi::{c_char, c_int, c_void};

pub(super) type DnsRef = *mut c_void;
pub(super) type DnsError = i32;
pub(super) const FLAGS_ADD: u32 = 0x2;
pub(super) const PROTOCOL_IPV4: u32 = 0x1;
pub(super) const AF_INET: u8 = 2;
type BrowseReply = unsafe extern "C" fn(
    DnsRef,
    u32,
    u32,
    DnsError,
    *const c_char,
    *const c_char,
    *const c_char,
    *mut c_void,
);
type ResolveReply = unsafe extern "C" fn(
    DnsRef,
    u32,
    u32,
    DnsError,
    *const c_char,
    *const c_char,
    u16,
    u16,
    *const u8,
    *mut c_void,
);
type AddrReply = unsafe extern "C" fn(
    DnsRef,
    u32,
    u32,
    DnsError,
    *const c_char,
    *const SockAddrIn,
    u32,
    *mut c_void,
);

#[repr(C)]
pub(super) struct SockAddrIn {
    pub(super) len: u8,
    pub(super) family: u8,
    pub(super) port: u16,
    pub(super) addr: [u8; 4],
    pub(super) zero: [u8; 8],
}
#[repr(C)]
pub(super) struct PollFd {
    pub(super) fd: c_int,
    pub(super) events: i16,
    pub(super) revents: i16,
}
pub(super) const POLLIN: i16 = 0x1;

unsafe extern "C" {
    pub(super) fn DNSServiceBrowse(
        r: *mut DnsRef,
        flags: u32,
        interface: u32,
        regtype: *const c_char,
        domain: *const c_char,
        reply: BrowseReply,
        context: *mut c_void,
    ) -> DnsError;
    pub(super) fn DNSServiceResolve(
        r: *mut DnsRef,
        flags: u32,
        interface: u32,
        name: *const c_char,
        regtype: *const c_char,
        domain: *const c_char,
        reply: ResolveReply,
        context: *mut c_void,
    ) -> DnsError;
    pub(super) fn DNSServiceGetAddrInfo(
        r: *mut DnsRef,
        flags: u32,
        interface: u32,
        protocol: u32,
        host: *const c_char,
        reply: AddrReply,
        context: *mut c_void,
    ) -> DnsError;
    pub(super) fn DNSServiceRefSockFD(r: DnsRef) -> c_int;
    pub(super) fn DNSServiceProcessResult(r: DnsRef) -> DnsError;
    pub(super) fn DNSServiceRefDeallocate(r: DnsRef);
    pub(super) fn TXTRecordGetValuePtr(
        len: u16,
        txt: *const u8,
        key: *const c_char,
        value_len: *mut u8,
    ) -> *const u8;
    pub(super) fn pipe(fds: *mut c_int) -> c_int;
    pub(super) fn write(fd: c_int, buf: *const c_void, n: usize) -> isize;
    pub(super) fn close(fd: c_int) -> c_int;
    pub(super) fn poll(fds: *mut PollFd, n: u32, timeout: c_int) -> c_int;
}
