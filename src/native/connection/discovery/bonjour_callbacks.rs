use super::*;

// C 字符串转 Rust 字符串，空指针得到空串
pub(super) fn text(value: *const c_char) -> String {
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
pub(super) unsafe extern "C" fn on_browse(
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
pub(super) fn begin_resolve(state: &mut State, key: String, service: Service) {
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
pub(super) fn txt_value(len: u16, txt: *const u8, key: &str) -> Option<String> {
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
pub(super) unsafe extern "C" fn on_resolve(
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
pub(super) unsafe extern "C" fn on_address(
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
