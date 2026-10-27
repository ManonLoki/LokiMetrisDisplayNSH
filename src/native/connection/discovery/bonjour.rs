//! iOS 系统 Bonjour（dns_sd）浏览器。经 mDNSResponder 发现服务，
//! 不需要多播 entitlement；Info.plist 须声明 NSBonjourServices 与本地网络用途。
//! 所有 DNSServiceRef 只在浏览线程创建、处理和释放。
use super::{super::SERVICE, Event, Found, resolved, valid_advertisement};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, CString, c_char, c_int, c_void},
    net::{IpAddr, Ipv4Addr},
    os::fd::RawFd,
    ptr,
    sync::mpsc::SyncSender,
    thread,
    time::{Duration, Instant},
};

#[path = "bonjour_dns.rs"]
mod dns;
use dns::{
    AF_INET, DNSServiceBrowse, DNSServiceGetAddrInfo, DNSServiceProcessResult,
    DNSServiceRefDeallocate, DNSServiceRefSockFD, DNSServiceResolve, DnsError, DnsRef, FLAGS_ADD,
    POLLIN, PROTOCOL_IPV4, PollFd, SockAddrIn, TXTRecordGetValuePtr, close, pipe, poll, write,
};

// A cold launch may race mDNSResponder/network readiness. Never retain a silent
// resolve forever: the owner rebuilds a failed browser with bounded backoff.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(8);

/// 一次解析：TXT 校验通过后再查 IPv4 地址。
struct Pending {
    state: *mut State,
    key: String,
    version: Option<String>,
    id: Option<String>,
    name: Option<String>,
    port: u16,
    resolve: DnsRef,
    address: DnsRef,
    deadline: Instant,
}

struct State {
    events: SyncSender<Event>,
    browse: DnsRef,
    pending: BTreeMap<String, Box<Pending>>,
    finished: Vec<String>,
    changes: BTreeMap<String, Option<Service>>,
    interfaces: BTreeMap<String, BTreeSet<u32>>,
    failure: Option<String>,
}

struct Service {
    interface: u32,
    name: CString,
    regtype: CString,
    domain: CString,
}

pub(super) struct Browser {
    wake: RawFd,
}
impl Browser {
    pub(super) fn start(events: SyncSender<Event>) -> Result<Self, String> {
        let mut fds = [0; 2];
        if unsafe { pipe(fds.as_mut_ptr()) } != 0 {
            return Err("局域网发现不可用，请检查本地网络权限。".into());
        }
        let (read, wake) = (fds[0], fds[1]);
        if thread::Builder::new()
            .name("loki-lan-browse".into())
            .spawn(move || run(events, read))
            .is_err()
        {
            unsafe {
                close(read);
                close(wake);
            }
            return Err("局域网发现不可用，请检查本地网络权限。".into());
        }
        Ok(Self { wake })
    }
    pub(super) fn stop(self) {
        unsafe {
            write(self.wake, [1u8].as_ptr().cast(), 1);
            close(self.wake);
        }
    }
}

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
    let context = (&mut *state as *mut State).cast::<c_void>();
    let regtype = CString::new(SERVICE.trim_end_matches(".local.")).unwrap_or_default();
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
    if status != 0 {
        let _ = state
            .events
            .send(Event::Discovery(Found::Failed(dns_error("Browse", status))));
        unsafe { close(wake) };
        return;
    }
    loop {
        let mut refs = vec![state.browse];
        for p in state.pending.values() {
            refs.extend([p.resolve, p.address].into_iter().filter(|r| !r.is_null()));
        }
        let mut polls: Vec<PollFd> = refs
            .iter()
            .map(|r| PollFd {
                fd: unsafe { DNSServiceRefSockFD(*r) },
                events: POLLIN,
                revents: 0,
            })
            .chain([PollFd {
                fd: wake,
                events: POLLIN,
                revents: 0,
            }])
            .collect();
        let timeout = state
            .pending
            .values()
            .map(|p| p.deadline)
            .min()
            .map_or(-1, |at| {
                at.saturating_duration_since(Instant::now())
                    .as_millis()
                    .min(c_int::MAX as u128) as c_int
            });
        if unsafe { poll(polls.as_mut_ptr(), polls.len() as u32, timeout) } < 0 {
            continue;
        }
        if polls.last().is_some_and(|p| p.revents != 0) {
            break;
        }
        for (r, p) in refs.iter().zip(&polls) {
            if p.revents != 0 {
                let status = unsafe { DNSServiceProcessResult(*r) };
                if status != 0 {
                    state.failure = Some(dns_error("ProcessResult", status));
                }
            }
        }
        for key in std::mem::take(&mut state.finished) {
            if let Some(p) = state.pending.remove(&key) {
                release(&p);
            }
        }
        // A ProcessResult batch can contain Remove followed by Add for the same
        // instance. Apply its final state only after every polled ref was used;
        // this preserves the Add and never frees a ref still in `refs`.
        for (key, service) in std::mem::take(&mut state.changes) {
            if let Some(pending) = state.pending.remove(&key) {
                release(&pending);
            }
            if let Some(service) = service {
                begin_resolve(&mut state, key, service);
            } else {
                let _ = state.events.send(Event::Discovery(Found::Removed(key)));
            }
        }
        state.expire(Instant::now());
        if let Some(message) = state.failure.take() {
            let _ = state.events.send(Event::Discovery(Found::Failed(message)));
            break;
        }
    }
    for p in std::mem::take(&mut state.pending).into_values() {
        release(&p);
    }
    unsafe {
        DNSServiceRefDeallocate(state.browse);
        close(wake);
    }
}

impl State {
    fn expire(&mut self, now: Instant) {
        if self.pending.values().any(|pending| pending.deadline <= now) {
            self.failure
                .get_or_insert_with(|| "Bonjour 解析超时，正在重新发现桌面端。".into());
        }
    }
}

fn dns_error(operation: &str, status: DnsError) -> String {
    // DNS stage/status are public diagnostics, never TXT data or credentials.
    format!("Bonjour {operation} 失败（{status}），正在重新发现桌面端。")
}

fn release(p: &Pending) {
    for r in [p.resolve, p.address] {
        if !r.is_null() {
            unsafe { DNSServiceRefDeallocate(r) };
        }
    }
}

fn text(value: *const c_char) -> String {
    if value.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned()
}

unsafe extern "C" fn on_browse(
    _: DnsRef,
    flags: u32,
    interface: u32,
    error: DnsError,
    name: *const c_char,
    regtype: *const c_char,
    domain: *const c_char,
    context: *mut c_void,
) {
    let state = unsafe { &mut *context.cast::<State>() };
    if error != 0 {
        state.failure = Some(dns_error("Browse", error));
        return;
    }
    let key = format!("{}.{}{}", text(name), text(regtype), text(domain));
    if !state.interfaces.contains_key(&key) && state.interfaces.len() >= 64 {
        return;
    }
    if !state.changes.contains_key(&key) && state.changes.len() >= 64 {
        return;
    }
    if flags & FLAGS_ADD == 0 {
        let Some(interfaces) = state.interfaces.get_mut(&key) else {
            return;
        };
        if !interfaces.remove(&interface) {
            return;
        }
        if interfaces.is_empty() {
            state.interfaces.remove(&key);
            state.changes.insert(key, None);
            return;
        }
    } else {
        let interfaces = state.interfaces.entry(key.clone()).or_default();
        if interfaces.len() >= 16 && !interfaces.contains(&interface) {
            return;
        }
        interfaces.insert(interface);
    }
    // Owner keys remain full service names, not per-interface aliases of the
    // same desktop. If one interface disappears, re-resolve a surviving one.
    let Some(&interface) = state.interfaces[&key].first() else {
        return;
    };
    let (Ok(name), Ok(regtype), Ok(domain)) = (
        CString::new(text(name)),
        CString::new(text(regtype)),
        CString::new(text(domain)),
    ) else {
        return;
    };
    state.changes.insert(
        key,
        Some(Service {
            interface,
            name,
            regtype,
            domain,
        }),
    );
}

fn begin_resolve(state: &mut State, key: String, service: Service) {
    if state.pending.len() >= 64 {
        return;
    }
    let mut pending = Box::new(Pending {
        state: state as *mut State,
        key: key.clone(),
        version: None,
        id: None,
        name: None,
        port: 0,
        resolve: ptr::null_mut(),
        address: ptr::null_mut(),
        deadline: Instant::now() + RESOLVE_TIMEOUT,
    });
    let context = (&mut *pending as *mut Pending).cast::<c_void>();
    let status = unsafe {
        DNSServiceResolve(
            &mut pending.resolve,
            0,
            service.interface,
            service.name.as_ptr(),
            service.regtype.as_ptr(),
            service.domain.as_ptr(),
            on_resolve,
            context,
        )
    };
    if status == 0 {
        state.pending.insert(key, pending);
    } else {
        state.failure = Some(dns_error("Resolve", status));
    }
}

fn txt_value(len: u16, txt: *const u8, key: &str) -> Option<String> {
    let key = CString::new(key).ok()?;
    let mut size = 0u8;
    let value = unsafe { TXTRecordGetValuePtr(len, txt, key.as_ptr(), &mut size) };
    if value.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(value, size as usize) };
    String::from_utf8(bytes.to_vec()).ok()
}

unsafe extern "C" fn on_resolve(
    _: DnsRef,
    _: u32,
    interface: u32,
    error: DnsError,
    _: *const c_char,
    host: *const c_char,
    port: u16,
    txt_len: u16,
    txt: *const u8,
    context: *mut c_void,
) {
    let pending = unsafe { &mut *context.cast::<Pending>() };
    let state = unsafe { &mut *pending.state };
    if error != 0 {
        state.failure = Some(dns_error("Resolve", error));
        return;
    }
    if !pending.address.is_null() {
        return;
    }
    pending.version = txt_value(txt_len, txt, "v");
    pending.id = txt_value(txt_len, txt, "id");
    pending.name = txt_value(txt_len, txt, "name");
    pending.port = u16::from_be(port);
    // Invalid advertisements must not cause a permanent retry loop for every
    // other service. Reject their TXT before querying an address.
    if !valid_advertisement(
        pending.version.as_deref(),
        pending.id.as_deref(),
        pending.name.as_deref(),
        pending.port,
    ) {
        state.finished.push(pending.key.clone());
        state.interfaces.remove(&pending.key);
        return;
    }
    let status = unsafe {
        DNSServiceGetAddrInfo(
            &mut pending.address,
            0,
            interface,
            PROTOCOL_IPV4,
            host,
            on_address,
            context,
        )
    };
    if status != 0 {
        pending.address = ptr::null_mut();
        state.failure = Some(dns_error("GetAddrInfo", status));
    }
}

unsafe extern "C" fn on_address(
    _: DnsRef,
    flags: u32,
    _: u32,
    error: DnsError,
    _: *const c_char,
    address: *const SockAddrIn,
    _: u32,
    context: *mut c_void,
) {
    let pending = unsafe { &mut *context.cast::<Pending>() };
    let state = unsafe { &mut *pending.state };
    if error != 0 {
        state.failure = Some(dns_error("GetAddrInfo", error));
        return;
    }
    // An initial negative/removal result is not a usable endpoint. Retain the
    // query until an Add arrives or the finite resolution deadline expires.
    if flags & FLAGS_ADD == 0 || address.is_null() {
        return;
    }
    let address = unsafe { &*address };
    if address.family != AF_INET {
        return;
    }
    let ip = IpAddr::V4(Ipv4Addr::from(address.addr));
    if let Some(found) = resolved(
        pending.key.clone(),
        pending.version.as_deref(),
        pending.id.as_deref(),
        pending.name.as_deref(),
        [ip],
        pending.port,
    ) {
        state.finished.push(pending.key.clone());
        let _ = state.events.send(Event::Discovery(found));
    }
}

#[cfg(test)]
#[path = "bonjour_callback_test.rs"]
mod callback_test;
