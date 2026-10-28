// 引入被测模块
use super::*;

// 测试用设备 ID
const DEVICE: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

// 只有握手而没有收到快照时，不算在线
#[test]
fn handshake_without_a_received_snapshot_is_not_online() {
    let initial = empty_view(DEVICE, "Phone");
    let mut raw = initial.clone();
    // 原生端口声称已连接且实时
    raw["connected"] = json!(true);
    raw["live"] = json!(true);
    // 没有快照，公开视图中仍是未连接
    let view = native_view(&raw.to_string()).unwrap();
    assert_eq!(view["connected"], false);
    assert_eq!(view["live"], false);
    // 有快照但没有接收时间，仍未连接
    raw["snapshot"] = json!({"version": 1, "tokens": null, "hooksAvailable": true, "hooks": []});
    assert_eq!(native_view(&raw.to_string()).unwrap()["connected"], false);
    // 快照加接收时间齐备才算已连接
    raw["receivedAtMs"] = json!(1);
    assert_eq!(native_view(&raw.to_string()).unwrap()["connected"], true);
}

// 非法的原生发布被拒绝，缓存数据保持离线
#[test]
fn invalid_native_publication_is_rejected_and_cached_data_stays_offline() {
    let initial = empty_view(DEVICE, "Phone");
    let mut raw = initial.clone();
    // 版本不对的快照被拒绝
    raw["snapshot"] = json!({"version": 9});
    assert!(native_view(&raw.to_string()).is_err());
    raw["snapshot"] = json!({"version": 1, "tokens": null, "hooksAvailable": true, "hooks": []});
    raw["receivedAtMs"] = json!(1);
    // 缓存回放：有快照与时间，但未连接
    raw["connected"] = json!(false);
    raw["live"] = json!(false);
    let cached = native_view(&raw.to_string()).unwrap();
    // 缓存的快照仍可展示
    assert!(!cached["snapshot"].is_null());
    assert_eq!(cached["connected"], false);
    // 全零设备 ID 被拒绝
    raw["deviceId"] = json!("00000000-0000-0000-0000-000000000000");
    assert!(native_view(&raw.to_string()).is_err());
}

// 已配对记录保留原生保存的时间
#[test]
fn paired_records_keep_the_native_save_time() {
    let host = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    let mut raw = empty_view(DEVICE, "Phone");
    raw["hosts"] = json!([{"id": host, "name": "Desk", "paired": true, "available": true}]);
    raw["paired"] = json!({host: {"clientId": host, "savedAtMs": 1_700_000_000_000u64}});
    let view = native_view(&raw.to_string()).unwrap();
    assert_eq!(view["paired"][host]["savedAtMs"], 1_700_000_000_000u64);
}
