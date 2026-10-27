use super::*;
use crate::native::connection::{
    Material,
    connection_test::{CLIENT, amount, snapshot},
    framed_socket::FramedSocket,
};
use serde_json::json;
use std::{collections::BTreeMap, net::TcpListener, sync::mpsc};

#[test]
fn authenticated_widget_grants_use_socket_peer_and_never_enter_public_view() {
    let root = std::env::temp_dir().join(format!("loki-widget-owner-{}", uuid::Uuid::new_v4()));
    let shared = root.join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = mpsc::sync_channel(256);
    let mut owner = Owner::for_test(
        root.join("link"),
        &id,
        "Widget fixture",
        "widget-fixture",
        tx.clone(),
    );
    owner.watch_companion = false;
    owner.widget = WidgetShare::new(Some(shared.clone()));
    owner.selected = Some(CLIENT.into());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    owner.socket = Some(FramedSocket::connect(
        listener.local_addr().unwrap(),
        false,
        1,
        tx,
    ));
    let message = json!({"type":"snapshot", "snapshot":snapshot(vec![]),
        "capabilities":["widgetHttpV1"], "widgetEndpoint":{"version":1,"port":51303,
        "key":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","host":"evil.example"}});
    owner.share_widget_snapshot(&message, 1000);
    let store = Store::new(&shared);
    assert!(
        store.load_target().unwrap().is_none(),
        "untrusted source cannot issue a Widget grant"
    );
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
    owner.share_widget_snapshot(&message, 1000);
    let target = store.load_target().unwrap().unwrap();
    assert_eq!(
        target["host"], "127.0.0.1",
        "only the authenticated socket supplies the address"
    );
    assert_eq!(target["deviceId"], id);
    let first = store.load().unwrap().unwrap();
    assert_eq!(first["receivedAtMs"], 1000);
    assert_eq!(first["validUntilMs"], 46000);
    owner.view["snapshot"] = message["snapshot"].clone();
    let public = owner.published_view().to_string();
    assert!(
        !public.contains("widgetEndpoint")
            && !public.contains("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
    );
    owner.clear_live();
    assert!(
        store.load_target().unwrap().is_some(),
        "pausing preserves independent HTTP authorization"
    );
    owner.forget_widget_source(CLIENT).unwrap();
    assert!(store.load_target().unwrap().is_none() && store.load().unwrap().is_none());
    assert!(
        !store.save_fetched(&first, &target, 1000).unwrap(),
        "a late HTTP result cannot revive a forgotten source"
    );
    drop(owner);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn widget_summary_uses_the_same_unicode_duplicate_admission_as_the_application() {
    let mut raw = snapshot(vec![]);
    raw["tokens"]["agents"] =
        json!([{"agent":"Café","amount":amount("1")},{"agent":"Cafe\u{301}","amount":amount("2")}]);
    assert!(crate::features::connection::snapshot::project_snapshot(&raw, &json!({})).is_none());
    assert!(summary::from_message(&raw, CLIENT, 1000).is_none());
    let mut raw = snapshot(vec![]);
    raw["enabledAgents"] = json!(["claudeCode", "CLAUDE_code"]);
    assert!(summary::from_message(&raw, CLIENT, 1000).is_none());
}

#[test]
fn owner_startup_revokes_shared_targets_no_longer_in_its_pairing_book() {
    let root = std::env::temp_dir().join(format!("loki-widget-revoke-{}", uuid::Uuid::new_v4()));
    let shared = root.join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = mpsc::sync_channel(256);
    let mut owner = Owner::for_test(
        root.join("link"),
        &id,
        "Widget fixture",
        "widget-fixture",
        tx,
    );
    owner.records = BTreeMap::new();
    owner.widget = WidgetShare::new(Some(shared.clone()));
    let target = widget_http::target(
        CLIENT,
        &id,
        "127.0.0.1",
        1234,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    )
    .unwrap();
    let record = summary::from_message(&snapshot(vec![]), CLIENT, 1000).unwrap();
    let store = Store::new(&shared);
    store.save_authenticated(&record, Some(&target)).unwrap();
    owner.reconcile_widget_authorization();
    assert!(store.load_target().unwrap().is_none() && store.load().unwrap().is_none());
    drop(owner);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_shared_revocation_cannot_report_forget_or_source_switch_success() {
    use crate::native::connection::{Command, Endpoint};
    let root = std::env::temp_dir().join(format!("loki-widget-failure-{}", uuid::Uuid::new_v4()));
    let shared = root.join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, _rx) = mpsc::sync_channel(256);
    let mut owner = Owner::for_test(
        root.join("link"),
        &id,
        "Widget fixture",
        "widget-failure-fixture",
        tx,
    );
    owner.watch_companion = false;
    owner.selected = Some(CLIENT.into());
    owner.view["selectedId"] = json!(CLIENT);
    owner.records.insert(
        CLIENT.into(),
        Material {
            client_id: CLIENT.into(),
            device_id: id.clone(),
            name: "Fixture".into(),
            secret: "private test book".into(),
            saved_at_ms: 1,
        },
    );
    owner.widget = WidgetShare::new(Some(shared.clone()));
    let target = widget_http::target(
        CLIENT,
        &id,
        "127.0.0.1",
        1234,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    )
    .unwrap();
    let record = summary::from_message(&snapshot(vec![]), CLIENT, 1000).unwrap();
    Store::new(&shared)
        .save_authenticated(&record, Some(&target))
        .unwrap();
    let unavailable = root.join("withheld");
    std::fs::rename(&shared, &unavailable).unwrap();
    let forgotten = owner.command(Command::Forget {
        client_id: CLIENT.into(),
    });
    assert!(
        forgotten.is_err(),
        "forget must fail when the extension grant cannot be revoked"
    );
    assert!(
        owner.records.contains_key(CLIENT),
        "a failed forget must retain the pairing book"
    );
    assert_eq!(owner.selected.as_deref(), Some(CLIENT));
    let other = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    owner.endpoints.insert(
        "fixture".into(),
        Endpoint {
            id: other.into(),
            name: "Other fixture".into(),
            address: "127.0.0.1:1234".parse().unwrap(),
        },
    );
    let switched = owner.command(Command::Select {
        client_id: other.into(),
    });
    assert!(
        switched.is_err(),
        "source selection must fail when the old Widget source cannot be revoked"
    );
    assert_eq!(owner.selected.as_deref(), Some(CLIENT));
    assert_eq!(owner.view["selectedId"], CLIENT);
    std::fs::rename(unavailable, &shared).unwrap();
    assert_eq!(Store::new(&shared).load_target().unwrap().unwrap(), target);
    assert_eq!(Store::new(&shared).load().unwrap().unwrap(), record);
    drop(owner);
    std::fs::remove_dir_all(root).unwrap();
}
