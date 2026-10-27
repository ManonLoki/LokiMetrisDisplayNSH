use super::*;
use std::sync::mpsc::{self, Receiver};

const KEY: &str = "cold._loki-host._tcp.local.";
const ID: &str = "abababab-abab-4aba-8aba-abababababab";

fn state() -> (Box<State>, Receiver<Event>) {
    let (events, receiver) = mpsc::sync_channel(16);
    (
        Box::new(State {
            events,
            browse: ptr::null_mut(),
            pending: BTreeMap::new(),
            finished: Vec::new(),
            changes: BTreeMap::new(),
            interfaces: BTreeMap::new(),
            failure: None,
        }),
        receiver,
    )
}

fn pending(state: &mut State) -> *mut c_void {
    let mut pending = Box::new(Pending {
        state,
        key: KEY.into(),
        version: Some("1".into()),
        id: Some(ID.into()),
        name: Some("冷启动回归".into()),
        port: 47321,
        resolve: ptr::null_mut(),
        address: ptr::null_mut(),
        deadline: Instant::now() + RESOLVE_TIMEOUT,
    });
    let context = (&mut *pending as *mut Pending).cast();
    state.pending.insert(KEY.into(), pending);
    state.interfaces.insert(KEY.into(), BTreeSet::from([14]));
    context
}

#[test]
fn resolve_error_reaches_the_browser_retry_path_without_reading_undefined_arguments() {
    let (mut state, _) = state();
    let context = pending(&mut state);
    unsafe {
        on_resolve(
            ptr::null_mut(),
            0,
            0,
            -65563,
            ptr::null(),
            ptr::null(),
            0,
            0,
            ptr::null(),
            context,
        )
    };
    assert_eq!(
        state.failure.as_deref(),
        Some("Bonjour Resolve 失败（-65563），正在重新发现桌面端。")
    );
}

#[test]
fn address_error_reaches_the_browser_retry_path() {
    let (mut state, _) = state();
    let context = pending(&mut state);
    unsafe {
        on_address(
            ptr::null_mut(),
            0,
            0,
            -65554,
            ptr::null(),
            ptr::null(),
            0,
            context,
        )
    };
    assert!(
        state
            .failure
            .as_deref()
            .unwrap()
            .contains("GetAddrInfo 失败（-65554）")
    );
}

#[test]
fn a_remove_or_unusable_address_does_not_consume_the_only_resolution() {
    let (mut state, receiver) = state();
    let context = pending(&mut state);
    let mut address = SockAddrIn {
        len: 16,
        family: AF_INET,
        port: 0,
        addr: [10, 0, 0, 221],
        zero: [0; 8],
    };
    unsafe { on_address(ptr::null_mut(), 0, 0, 0, ptr::null(), &address, 0, context) };
    assert!(state.finished.is_empty());
    assert!(receiver.try_recv().is_err());
    address.addr = [8, 8, 8, 8];
    unsafe {
        on_address(
            ptr::null_mut(),
            FLAGS_ADD,
            0,
            0,
            ptr::null(),
            &address,
            0,
            context,
        )
    };
    assert!(state.finished.is_empty());
    address.addr = [10, 0, 0, 221];
    unsafe {
        on_address(
            ptr::null_mut(),
            FLAGS_ADD,
            0,
            0,
            ptr::null(),
            &address,
            0,
            context,
        )
    };
    assert_eq!(state.finished, [KEY]);
    assert!(
        matches!(receiver.try_recv().unwrap(), Event::Discovery(Found::Resolved { id, address, .. })
        if id == ID && address.to_string() == "10.0.0.221:47321")
    );
}

#[test]
fn a_silent_resolution_has_a_finite_retry_deadline() {
    let (mut state, _) = state();
    pending(&mut state);
    let deadline = state.pending[KEY].deadline;
    state.expire(deadline - Duration::from_millis(1));
    assert!(state.failure.is_none());
    state.expire(deadline);
    assert!(state.failure.as_deref().unwrap().contains("解析超时"));
}

#[test]
fn remove_then_add_in_one_process_result_retains_the_latest_add_until_safe_cleanup() {
    let (mut state, receiver) = state();
    pending(&mut state);
    let name = CString::new("cold").unwrap();
    let regtype = CString::new("_loki-host._tcp.").unwrap();
    let domain = CString::new("local.").unwrap();
    let context = (&mut *state as *mut State).cast();
    for flags in [0, FLAGS_ADD] {
        unsafe {
            on_browse(
                ptr::null_mut(),
                flags,
                14,
                0,
                name.as_ptr(),
                regtype.as_ptr(),
                domain.as_ptr(),
                context,
            )
        };
    }
    assert_eq!(state.changes[KEY].as_ref().unwrap().interface, 14);
    assert!(
        state.pending.contains_key(KEY),
        "polled references stay alive until batch cleanup"
    );
    assert!(
        receiver.try_recv().is_err(),
        "coalesced Remove must not erase the new Add"
    );
}

#[test]
fn invalid_txt_is_rejected_without_querying_an_address_or_poisoning_other_services() {
    let (mut state, _) = state();
    let context = pending(&mut state);
    let txt = [3, b'v', b'=', b'9'];
    unsafe {
        on_resolve(
            ptr::null_mut(),
            0,
            14,
            0,
            ptr::null(),
            ptr::null(),
            47321_u16.to_be(),
            txt.len() as u16,
            txt.as_ptr(),
            context,
        )
    };
    assert_eq!(state.finished, [KEY]);
    assert!(state.failure.is_none());
    assert!(state.pending[KEY].address.is_null());
    assert!(!state.interfaces.contains_key(KEY));
}

#[test]
fn removing_one_interface_keeps_the_same_desktop_until_the_last_interface_is_removed() {
    let (mut state, _) = state();
    let name = CString::new("cold").unwrap();
    let regtype = CString::new("_loki-host._tcp.").unwrap();
    let domain = CString::new("local.").unwrap();
    let context = (&mut *state as *mut State).cast();
    for (flags, interface) in [(FLAGS_ADD, 14), (FLAGS_ADD, 17), (0, 17)] {
        unsafe {
            on_browse(
                ptr::null_mut(),
                flags,
                interface,
                0,
                name.as_ptr(),
                regtype.as_ptr(),
                domain.as_ptr(),
                context,
            )
        };
    }
    assert_eq!(
        state.changes.len(),
        1,
        "multiple interfaces remain one owner identity"
    );
    assert_eq!(state.changes[KEY].as_ref().unwrap().interface, 14);
    unsafe {
        on_browse(
            ptr::null_mut(),
            0,
            14,
            0,
            name.as_ptr(),
            regtype.as_ptr(),
            domain.as_ptr(),
            context,
        )
    };
    assert!(state.changes[KEY].is_none());
    assert!(!state.interfaces.contains_key(KEY));
}
