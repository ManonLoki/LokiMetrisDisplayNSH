// 引入被测模块
use super::*;
// 引入配对材料、测试夹具与套接字
use crate::native::connection::{
    Material,
    connection_test::{CLIENT, amount, snapshot},
    framed_socket::FramedSocket,
};
use serde_json::json;
use std::{collections::BTreeMap, net::TcpListener, sync::mpsc};

#[test]
// 认证后的小组件授权使用套接字对端地址，且绝不进入公开视图
fn authenticated_widget_grants_use_socket_peer_and_never_enter_public_view() {
    // 独立的临时根目录
    let root = std::env::temp_dir().join(format!("loki-widget-owner-{}", uuid::Uuid::new_v4()));
    // 模拟 App Group 共享目录
    let shared = root.join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    // 随机的本机设备 ID
    let id = uuid::Uuid::new_v4().to_string();
    // 事件队列（接收端不使用）
    let (tx, _rx) = mpsc::sync_channel(256);
    // 构造测试用 owner
    let mut owner = Owner::for_test(
        root.join("link"),
        &id,
        "Widget fixture",
        "widget-fixture",
        tx.clone(),
    );
    // 不发布手表缓存
    owner.watch_companion = false;
    // 接上共享存储
    owner.widget = WidgetShare::new(Some(shared.clone()));
    // 选中测试桌面
    owner.selected = Some(CLIENT.into());
    // 本机监听一个端口，让套接字有真实的对端地址
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    owner.socket = Some(FramedSocket::connect(
        listener.local_addr().unwrap(),
        false,
        1,
        tx,
    ));
    // 带有恶意 host 的快照消息：声明了 widgetHttpV1 能力
    let message = json!({"type":"snapshot", "snapshot":snapshot(vec![]),
        "capabilities":["widgetHttpV1"], "widgetEndpoint":{"version":1,"port":51303,
        "key":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","host":"evil.example"}});
    // 此时桌面还没有配对记录
    owner.share_widget_snapshot(&message, 1000);
    let store = Store::new(&shared);
    // 未受信任的来源不能签发小组件授权
    assert!(
        store.load_target().unwrap().is_none(),
        "untrusted source cannot issue a Widget grant"
    );
    // 加入配对记录，使该桌面变为受信任
    owner.records.insert(
        CLIENT.into(),
        Material {
            client_id: CLIENT.into(),
            device_id: id.clone(),
            name: "Fixture".into(),
            secret: "private pairing book".into(),
            saved_at_ms: 1,
        },
    );
    // 再次共享
    owner.share_widget_snapshot(&message, 1000);
    let target = store.load_target().unwrap().unwrap();
    // 地址只来自已认证的套接字，不采信消息里的 host
    assert_eq!(
        target["host"], "127.0.0.1",
        "only the authenticated socket supplies the address"
    );
    assert_eq!(target["deviceId"], id);
    // 摘要的接收时间与 45 秒租约
    let first = store.load().unwrap().unwrap();
    assert_eq!(first["receivedAtMs"], 1000);
    assert_eq!(first["validUntilMs"], 46000);
    // 公开视图不得包含小组件端点或读取密钥
    owner.view["snapshot"] = message["snapshot"].clone();
    let public = owner.published_view().to_string();
    assert!(
        !public.contains("widgetEndpoint")
            && !public.contains("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
    );
    // 暂停连接后，独立的 HTTP 授权仍保留
    owner.clear_live();
    assert!(
        store.load_target().unwrap().is_some(),
        "pausing preserves independent HTTP authorization"
    );
    // 忘记来源后，凭据和摘要都被清掉
    owner.forget_widget_source(CLIENT).unwrap();
    assert!(store.load_target().unwrap().is_none() && store.load().unwrap().is_none());
    // 迟到的 HTTP 结果不能让已忘记的来源复活
    assert!(
        !store.save_fetched(&first, &target, 1000).unwrap(),
        "a late HTTP result cannot revive a forgotten source"
    );
    drop(owner);
    // 清理
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
// 小组件摘要与应用使用同一套 Unicode 重复名称的准入规则
fn widget_summary_uses_the_same_unicode_duplicate_admission_as_the_application() {
    // 构造快照：两个代理名在 Unicode 归一化后相同
    let mut raw = snapshot(vec![]);
    raw["tokens"]["agents"] =
        json!([{"agent":"Café","amount":amount("1")},{"agent":"Cafe\u{301}","amount":amount("2")}]);
    // 应用的投影拒绝
    assert!(crate::features::connection::snapshot::project_snapshot(&raw, &json!({})).is_none());
    // 小组件摘要同样拒绝
    assert!(summary::from_message(&raw, CLIENT, 1000).is_none());
    // 启用列表里的两个名称归一化后重复
    let mut raw = snapshot(vec![]);
    raw["enabledAgents"] = json!(["claudeCode", "CLAUDE_code"]);
    // 同样被拒绝
    assert!(summary::from_message(&raw, CLIENT, 1000).is_none());
}

#[test]
// owner 启动时会撤销配对簿里已不存在的共享凭据
fn owner_startup_revokes_shared_targets_no_longer_in_its_pairing_book() {
    let root = std::env::temp_dir().join(format!("loki-widget-revoke-{}", uuid::Uuid::new_v4()));
    let shared = root.join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = mpsc::sync_channel(256);
    let mut owner = Owner::for_test(
        root.join("link"),
        // 标识
        &id,
        // 名称
        "Widget fixture",
        // 目录名
        "widget-fixture",
        // 事件通道
        tx,
    );
    // owner 的配对簿是空的
    owner.records = BTreeMap::new();
    owner.widget = WidgetShare::new(Some(shared.clone()));
    // 但共享存储里却有这个桌面的凭据与摘要
    let target = widget_http::target(
        CLIENT,
        &id,
        "127.0.0.1",
        1234,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    )
    .unwrap();
    // 由快照构造摘要
    let record = summary::from_message(&snapshot(vec![]), CLIENT, 1000).unwrap();
    // 打开共享存储
    let store = Store::new(&shared);
    // 以已认证身份保存
    store.save_authenticated(&record, Some(&target)).unwrap();
    // 启动对账
    owner.reconcile_widget_authorization();
    // 凭据与摘要都被撤销
    assert!(store.load_target().unwrap().is_none() && store.load().unwrap().is_none());
    drop(owner);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
// 共享撤销失败时，不能报告忘记或切换来源成功
fn failed_shared_revocation_cannot_report_forget_or_source_switch_success() {
    use crate::native::connection::{Command, Endpoint};
    let root = std::env::temp_dir().join(format!("loki-widget-failure-{}", uuid::Uuid::new_v4()));
    let shared = root.join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = mpsc::sync_channel(256);
    let mut owner = Owner::for_test(
        // 链接目录
        root.join("link"),
        // 标识
        &id,
        // 名称
        "Widget fixture",
        // 目录名
        "widget-failure-fixture",
        // 事件通道
        tx,
    );
    // 关闭手表配套
    owner.watch_companion = false;
    // 已选中并已配对的桌面
    owner.selected = Some(CLIENT.into());
    owner.view["selectedId"] = json!(CLIENT);
    owner.records.insert(
        CLIENT.into(),
        Material {
            client_id: CLIENT.into(),
            device_id: id.clone(),
            name: "Fixture".into(),
            // 测试用密钥
            secret: "private test book".into(),
            // 保存时间
            saved_at_ms: 1,
        },
    );
    // 设置小组件共享
    owner.widget = WidgetShare::new(Some(shared.clone()));
    // 共享存储里已有该桌面的凭据与摘要
    let target = widget_http::target(
        CLIENT,
        &id,
        "127.0.0.1",
        1234,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    )
    .unwrap();
    // 由快照构造摘要
    let record = summary::from_message(&snapshot(vec![]), CLIENT, 1000).unwrap();
    // 打开共享存储
    Store::new(&shared)
        // 以已认证身份保存
        .save_authenticated(&record, Some(&target))
        // 保存成功
        .unwrap();
    // 把共享目录挪走，模拟撤销时存储不可用
    let unavailable = root.join("withheld");
    std::fs::rename(&shared, &unavailable).unwrap();
    // 此时忘记桌面
    let forgotten = owner.command(Command::Forget {
        client_id: CLIENT.into(),
    });
    // 撤销不了授权就必须失败
    assert!(
        forgotten.is_err(),
        "forget must fail when the extension grant cannot be revoked"
    );
    // 失败的忘记要保留配对簿
    assert!(
        owner.records.contains_key(CLIENT),
        "a failed forget must retain the pairing book"
    );
    assert_eq!(owner.selected.as_deref(), Some(CLIENT));
    // 再发现另一台桌面
    let other = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    owner.endpoints.insert(
        "fixture".into(),
        Endpoint {
            id: other.into(),
            name: "Other fixture".into(),
            address: "127.0.0.1:1234".parse().unwrap(),
        },
    );
    // 切换来源同样会因无法撤销旧来源而失败
    let switched = owner.command(Command::Select {
        client_id: other.into(),
    });
    assert!(
        switched.is_err(),
        "source selection must fail when the old Widget source cannot be revoked"
    );
    // 选择保持不变
    assert_eq!(owner.selected.as_deref(), Some(CLIENT));
    assert_eq!(owner.view["selectedId"], CLIENT);
    // 把目录放回去，原有授权与摘要毫发无损
    std::fs::rename(unavailable, &shared).unwrap();
    assert_eq!(Store::new(&shared).load_target().unwrap().unwrap(), target);
    assert_eq!(Store::new(&shared).load().unwrap().unwrap(), record);
    drop(owner);
    std::fs::remove_dir_all(root).unwrap();
}
