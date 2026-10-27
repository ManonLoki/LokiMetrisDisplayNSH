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
fn imported_names_preserve_their_original_blank_contracts() {
    assert!(name_error("\u{feff}").is_some());
    assert!(apple_name_error("\u{feff}").is_none());
    assert!(android_name_error("\u{feff}").is_none());
    assert_eq!(apple_name_error("\u{85}"), Some("请输入设备名称"));
    assert_eq!(
        android_name_error("\u{85}"),
        Some("设备名称不能包含控制字符")
    );
    assert_eq!(android_name_error("\u{1c}"), Some("请输入设备名称"));
    assert_eq!(apple_name_error("\u{200b}"), Some("请输入设备名称"));
    assert!(name_error("\u{200b}").is_none());
}

#[test]
fn names_are_limited_by_utf8_bytes_without_changing_saved_text() {
    assert!(name_error(&"界".repeat(53)).is_none());
    assert!(name_error(&"界".repeat(54)).is_some());
    assert_eq!(android_default_name("  客厅平板  "), "客厅平板");
    assert_eq!(android_default_name("\u{85}"), "Android 显示端");
}

#[test]
fn uuid_validation_preserves_canonical_nonzero_contract() {
    assert!(valid_device_id("11111111-1111-1111-1111-111111111111"));
    assert!(!valid_device_id("00000000-0000-0000-0000-000000000000"));
    assert!(!valid_device_id("AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA"));
    assert!(!valid_device_id("../../identity"));
}
