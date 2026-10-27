use super::*;

const DEVICE: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

#[test]
fn handshake_without_a_received_snapshot_is_not_online() {
    let initial = empty_view(DEVICE, "Phone");
    let mut raw = initial.clone();
    raw["connected"] = json!(true);
    raw["live"] = json!(true);
    let view = native_view(&raw.to_string()).unwrap();
    assert_eq!(view["connected"], false);
    assert_eq!(view["live"], false);
    raw["snapshot"] = json!({"version": 1, "tokens": null, "hooksAvailable": true, "hooks": []});
    assert_eq!(native_view(&raw.to_string()).unwrap()["connected"], false);
    raw["receivedAtMs"] = json!(1);
    assert_eq!(native_view(&raw.to_string()).unwrap()["connected"], true);
}

#[test]
fn invalid_native_publication_is_rejected_and_cached_data_stays_offline() {
    let initial = empty_view(DEVICE, "Phone");
    let mut raw = initial.clone();
    raw["snapshot"] = json!({"version": 9});
    assert!(native_view(&raw.to_string()).is_err());
    raw["snapshot"] = json!({"version": 1, "tokens": null, "hooksAvailable": true, "hooks": []});
    raw["receivedAtMs"] = json!(1);
    raw["connected"] = json!(false);
    raw["live"] = json!(false);
    let cached = native_view(&raw.to_string()).unwrap();
    assert!(!cached["snapshot"].is_null());
    assert_eq!(cached["connected"], false);
    raw["deviceId"] = json!("00000000-0000-0000-0000-000000000000");
    assert!(native_view(&raw.to_string()).is_err());
}

#[test]
fn paired_records_keep_the_native_save_time() {
    let host = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    let mut raw = empty_view(DEVICE, "Phone");
    raw["hosts"] = json!([{"id": host, "name": "Desk", "paired": true, "available": true}]);
    raw["paired"] = json!({host: {"clientId": host, "savedAtMs": 1_700_000_000_000u64}});
    let view = native_view(&raw.to_string()).unwrap();
    assert_eq!(view["paired"][host]["savedAtMs"], 1_700_000_000_000u64);
}
