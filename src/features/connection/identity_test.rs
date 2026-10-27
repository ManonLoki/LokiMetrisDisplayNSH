use super::*;

#[test]
fn names_use_utf8_limits_and_reject_invisible_or_control_only_names() {
    assert_eq!(name_error(&"a".repeat(160)), None);
    assert!(name_error(&"a".repeat(161)).is_some());
    assert_eq!(name_error(&"界".repeat(53)), None);
    assert!(name_error(&"界".repeat(54)).is_some());
    assert_eq!(name_error("\u{feff}\u{00a0}"), Some("请输入设备名称"));
    assert_eq!(name_error("Phone\u{85}"), Some("设备名称不能包含控制字符"));
}

#[test]
fn placeholder_or_noncanonical_identity_cannot_commit_a_name() {
    let real = "12345678-abcd-1234-aaaa-123456789abc";
    assert!(valid_device_id(real));
    assert!(!valid_device_id(ZERO_DEVICE_ID));
    assert!(!valid_device_id("12345678-ABCD-1234-aaaa-123456789abc"));
    assert_eq!(commit_device_name("Phone", ZERO_DEVICE_ID)["ok"], false);
    assert_eq!(
        commit_device_name("Phone", real),
        json!({"ok": true, "name": "Phone"})
    );
}
