use super::*;

// C 字符串转 Rust 字符串，空指针得到空串
pub(super) fn text(value: *const c_char) -> String {
    if value.is_null() {
        return String::new();
    }
    // 按 C 字符串读取
    unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned()
}

// 浏览回调：记录服务在各网卡上的出现与消失
pub(super) unsafe extern "C" fn on_browse(
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
    // 服务数与待处理变化数各有 64 的上限
    if !state.interfaces.contains_key(&key) && state.interfaces.len() >= 64 {
        return;
    }
    // 变更队列已满且是新键
    if !state.changes.contains_key(&key) && state.changes.len() >= 64 {
        return;
    }
    // 消失：从该网卡移除；所有网卡都没了才算真正移除
    if flags & FLAGS_ADD == 0 {
        // 取该服务的网卡集合
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
    // 出现：记录网卡，每个服务最多 16 个
    } else {
        // 取或建该服务的网卡集合
        let interfaces = state.interfaces.entry(key.clone()).or_default();
        // 网卡数达到上限且是新网卡
        if interfaces.len() >= 16 && !interfaces.contains(&interface) {
            return;
        }
        interfaces.insert(interface);
    }
    // 所有者键仍是完整的服务名,而不是同一台桌面的各网卡别名。
    // 如果某个网卡消失,就重新解析仍然存活的那个。
    // owner 的键始终是完整服务名，而不是每网卡的别名；某网卡消失时改用留下的那一个重新解析
    let Some(&interface) = state.interfaces[&key].first() else {
        return;
    };
    // 名称、类型与域都必须合法
    let (Ok(name), Ok(regtype), Ok(domain)) = (
        // 服务名转 C 字符串
        CString::new(text(name)),
        // 服务类型转 C 字符串
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

pub(super) fn begin_resolve(state: &mut State, key: String, service: Service) {
    // 解析并发上限 64
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
    // 回调上下文指向待解析项
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
    // 成功则登记，否则记为失败
    if status == 0 {
        state.pending.insert(key, pending);
    } else {
        state.failure = Some(dns_error("Resolve", status));
    }
}

// 读取 TXT 中的 UTF-8 值
pub(super) fn txt_value(len: u16, txt: *const u8, key: &str) -> Option<String> {
    // 键转为 C 字符串
    let key = CString::new(key).ok()?;
    let mut size = 0u8;
    // 在 TXT 记录里查找该键
    let value = unsafe { TXTRecordGetValuePtr(len, txt, key.as_ptr(), &mut size) };
    if value.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(value, size as usize) };
    // 转为字符串，非法则 None
    String::from_utf8(bytes.to_vec()).ok()
}

// 解析回调：读取 TXT，校验通过再查 IPv4 地址
pub(super) unsafe extern "C" fn on_resolve(
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
    // 已在查地址则忽略重复回调
    if !pending.address.is_null() {
        return;
    }
    // 读取协议版本、设备 ID、名称与端口
    pending.version = txt_value(txt_len, txt, "v");
    pending.id = txt_value(txt_len, txt, "id");
    pending.name = txt_value(txt_len, txt, "name");
    pending.port = u16::from_be(port);
    // 无效的通告不得导致对其他每个服务的永久重试循环。
    // 在查询地址之前就拒绝它们的 TXT。
    // 非法的广告不能让其他服务陷入永久重试：地址查询前就拒绝其 TXT
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
    // 发起 IPv4 地址查询
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

// 地址回调：拿到可用的 IPv4 就报告解析完成
pub(super) unsafe extern "C" fn on_address(
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
    // 最初的否定/移除结果不是可用的端点。保留该查询,
    // 直到 Add 到达或有限的解析期限届满。
    // 起初的否定或删除结果不是可用端点；保留查询直到出现新增结果或解析期限届满
    if flags & FLAGS_ADD == 0 || address.is_null() {
        return;
    }
    let address = unsafe { &*address };
    // 只处理 IPv4
    if address.family != AF_INET {
        return;
    }
    // 由四字节构造 IPv4 地址
    let ip = IpAddr::V4(Ipv4Addr::from(address.addr));
    // 通过校验才算解析成功
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
