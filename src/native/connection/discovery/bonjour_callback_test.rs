// 引入被测模块
use super::*;
use std::sync::mpsc::{self, Receiver};

// 测试用服务实例全名
const KEY: &str = "cold._loki-host._tcp.local.";
// 测试用设备 ID
const ID: &str = "abababab-abab-4aba-8aba-abababababab";

// 构造浏览线程的状态
fn state() -> (Box<State>, Receiver<Event>) {
    // 事件通道
    let (events, receiver) = mpsc::sync_channel(16);
    (
        // 一个空白的浏览线程状态
        Box::new(State {
            events,
            browse: ptr::null_mut(),
            pending: BTreeMap::new(),
            finished: Vec::new(),
            changes: BTreeMap::new(),
            interfaces: BTreeMap::new(),
            failure: None,
        }),
        // 事件接收端
        receiver,
    )
}

// 构造一次待决解析，返回回调用的上下文指针
fn pending(state: &mut State) -> *mut c_void {
    // 构造待决解析：TXT 合法、端口 47321、期限为标准解析超时
    let mut pending = Box::new(Pending {
        state,
        key: KEY.into(),
        version: Some("1".into()),
        id: Some(ID.into()),
        name: Some("冷启动回归".into()),
        port: 47321,
        resolve: ptr::null_mut(),
        // 尚无地址查询
        address: ptr::null_mut(),
        // 解析期限
        deadline: Instant::now() + RESOLVE_TIMEOUT,
    });
    // 回调用的上下文指针
    let context = (&mut *pending as *mut Pending).cast();
    // 登记解析与它所在的网卡 14
    state.pending.insert(KEY.into(), pending);
    state.interfaces.insert(KEY.into(), BTreeSet::from([14]));
    context
}

#[test]
// 解析出错会进入浏览器重试路径，且不读取未定义的参数
fn resolve_error_reaches_the_browser_retry_path_without_reading_undefined_arguments() {
    let (mut state, _) = state();
    let context = pending(&mut state);
    // 以错误码 -65563 触发解析回调，其余参数都是空指针
    unsafe {
        on_resolve(
            ptr::null_mut(),
            0,
            0,
            -65563,
            ptr::null(),
            ptr::null(),
            // 标志 0
            0,
            // 网卡 0
            0,
            // 空记录
            ptr::null(),
            // 回调上下文
            context,
        )
    };
    // 失败被记录为终态错误，由浏览器的重试路径处理
    assert_eq!(
        state.failure.as_deref(),
        Some("Bonjour Resolve 失败（-65563），正在重新发现桌面端。")
    );
}

#[test]
// 地址查询出错同样进入重试路径
fn address_error_reaches_the_browser_retry_path() {
    let (mut state, _) = state();
    let context = pending(&mut state);
    unsafe {
        // 以错误码 -65554 触发地址回调
        on_address(
            ptr::null_mut(),
            0,
            0,
            -65554,
            ptr::null(),
            ptr::null(),
            0,
            // 回调上下文
            context,
        )
    };
    // 失败原因应包含错误码
    assert!(
        // 状态
        state
            // 失败信息
            .failure
            // 取字符串
            .as_deref()
            // 必有值
            .unwrap()
            // 包含 GetAddrInfo 失败（-65554）
            .contains("GetAddrInfo 失败（-65554）")
    );
}

#[test]
// 删除或不可用的地址不会耗掉唯一的一次解析
fn a_remove_or_unusable_address_does_not_consume_the_only_resolution() {
    let (mut state, receiver) = state();
    let context = pending(&mut state);
    // 一个私网 IPv4 地址
    let mut address = SockAddrIn {
        len: 16,
        family: AF_INET,
        port: 0,
        addr: [10, 0, 0, 221],
        zero: [0; 8],
    };
    // 不带“新增”标志：是删除结果，不算可用端点
    unsafe { on_address(ptr::null_mut(), 0, 0, 0, ptr::null(), &address, 0, context) };
    // 解析没有被消耗，也没有事件
    assert!(state.finished.is_empty());
    assert!(receiver.try_recv().is_err());
    // 换成公网地址并带“新增”：不可用，仍不消耗解析
    address.addr = [8, 8, 8, 8];
    unsafe {
        on_address(
            ptr::null_mut(),
            FLAGS_ADD,
            0,
            0,
            ptr::null(),
            // 地址
            &address,
            // 生存期 0
            0,
            // 回调上下文
            context,
        )
    };
    // 尚未完成
    assert!(state.finished.is_empty());
    // 再给私网地址并带“新增”：解析完成
    address.addr = [10, 0, 0, 221];
    unsafe {
        on_address(
            ptr::null_mut(),
            FLAGS_ADD,
            0,
            0,
            ptr::null(),
            // 地址
            &address,
            // 生存期 0
            0,
            // 回调上下文
            context,
        )
    };
    // 现在才完成
    assert_eq!(state.finished, [KEY]);
    // 并向 owner 报告解析出的端点
    assert!(
        matches!(receiver.try_recv().unwrap(), Event::Discovery(Found::Resolved { id, address, .. })
        if id == ID && address.to_string() == "10.0.0.221:47321")
    );
}

#[test]
// 长时间无响应的解析有有限的重试期限
fn a_silent_resolution_has_a_finite_retry_deadline() {
    let (mut state, _) = state();
    pending(&mut state);
    let deadline = state.pending[KEY].deadline;
    // 期限前 1 毫秒：不算失败
    state.expire(deadline - Duration::from_millis(1));
    assert!(state.failure.is_none());
    // 到期：记为解析超时
    state.expire(deadline);
    assert!(state.failure.as_deref().unwrap().contains("解析超时"));
}

#[test]
// 同一批结果里先删后增时，保留最新的新增，直到可以安全清理
fn remove_then_add_in_one_process_result_retains_the_latest_add_until_safe_cleanup() {
    let (mut state, receiver) = state();
    pending(&mut state);
    // 服务实例的名称、类型与域
    let name = CString::new("cold").unwrap();
    let regtype = CString::new("_loki-host._tcp.").unwrap();
    let domain = CString::new("local.").unwrap();
    let context = (&mut *state as *mut State).cast();
    // 同一批里先是删除、再是新增
    for flags in [0, FLAGS_ADD] {
        unsafe {
            on_browse(
                ptr::null_mut(),
                flags,
                14,
                0,
                name.as_ptr(),
                // 服务类型
                regtype.as_ptr(),
                // 域
                domain.as_ptr(),
                // 回调上下文
                context,
            )
        };
    }
    // 最终的变化是“新增”，网卡为 14
    assert_eq!(state.changes[KEY].as_ref().unwrap().interface, 14);
    // 正在轮询的引用要等整批处理完才释放
    assert!(
        state.pending.contains_key(KEY),
        "polled references stay alive until batch cleanup"
    );
    // 合并后的删除不能抹掉新的新增
    assert!(
        receiver.try_recv().is_err(),
        "coalesced Remove must not erase the new Add"
    );
}

#[test]
// 非法 TXT 被拒绝，既不查询地址，也不影响其他服务
fn invalid_txt_is_rejected_without_querying_an_address_or_poisoning_other_services() {
    let (mut state, _) = state();
    let context = pending(&mut state);
    // TXT 声明协议版本 9，不合法
    let txt = [3, b'v', b'=', b'9'];
    unsafe {
        on_resolve(
            ptr::null_mut(),
            0,
            14,
            0,
            ptr::null(),
            // 空主机名
            ptr::null(),
            // 端口（网络字节序）
            47321_u16.to_be(),
            // TXT 长度
            txt.len() as u16,
            // TXT 内容
            txt.as_ptr(),
            // 回调上下文
            context,
        )
    };
    // 这次解析被结束，但不是整体失败
    assert_eq!(state.finished, [KEY]);
    assert!(state.failure.is_none());
    // 没有发起地址查询
    assert!(state.pending[KEY].address.is_null());
    // 服务被忘记，不会反复重试
    assert!(!state.interfaces.contains_key(KEY));
}

#[test]
// 移除一个网卡时仍保留该桌面，直到最后一个网卡也被移除
fn removing_one_interface_keeps_the_same_desktop_until_the_last_interface_is_removed() {
    let (mut state, _) = state();
    let name = CString::new("cold").unwrap();
    let regtype = CString::new("_loki-host._tcp.").unwrap();
    let domain = CString::new("local.").unwrap();
    let context = (&mut *state as *mut State).cast();
    // 网卡 14、17 先后出现，随后 17 消失
    for (flags, interface) in [(FLAGS_ADD, 14), (FLAGS_ADD, 17), (0, 17)] {
        unsafe {
            on_browse(
                ptr::null_mut(),
                flags,
                interface,
                0,
                name.as_ptr(),
                // 服务类型
                regtype.as_ptr(),
                // 域
                domain.as_ptr(),
                // 回调上下文
                context,
            )
        };
    }
    // 多个网卡仍是同一个 owner 身份：只有一项变化
    assert_eq!(
        state.changes.len(),
        1,
        "multiple interfaces remain one owner identity"
    );
    // 改用留下的网卡 14 重新解析
    assert_eq!(state.changes[KEY].as_ref().unwrap().interface, 14);
    unsafe {
        // 最后一个网卡 14 也消失
        on_browse(
            ptr::null_mut(),
            0,
            14,
            0,
            name.as_ptr(),
            regtype.as_ptr(),
            domain.as_ptr(),
            // 回调上下文
            context,
        )
    };
    // 这时才算桌面真正消失
    assert!(state.changes[KEY].is_none());
    assert!(!state.interfaces.contains_key(KEY));
}
