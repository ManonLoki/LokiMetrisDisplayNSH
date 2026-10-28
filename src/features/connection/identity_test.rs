// 引入被测模块的全部内容
use super::*;

// 名称按 UTF-8 字节限制，并拒绝不可见字符或只含控制字符的名称
#[test]
fn names_use_utf8_limits_and_reject_invisible_or_control_only_names() {
    // 160 字节恰好合法，161 字节非法
    assert_eq!(name_error(&"a".repeat(160)), None);
    assert!(name_error(&"a".repeat(161)).is_some());
    // 每个汉字 3 字节：53 个合法，54 个超限
    assert_eq!(name_error(&"界".repeat(53)), None);
    assert!(name_error(&"界".repeat(54)).is_some());
    // 只有零宽与不换行空格视为空白名称
    assert_eq!(name_error("\u{feff}\u{00a0}"), Some("请输入设备名称"));
    // 含 C1 控制字符的名称被拒绝
    assert_eq!(name_error("Phone\u{85}"), Some("设备名称不能包含控制字符"));
}

// 移植来的名称保持各自平台的空白语义
#[test]
fn imported_names_preserve_their_original_blank_contracts() {
    // 共享规则下 BOM 视为空白
    assert!(name_error("\u{feff}").is_some());
    // Apple 与 Android 的规则下 BOM 不是空白
    assert!(apple_name_error("\u{feff}").is_none());
    assert!(android_name_error("\u{feff}").is_none());
    // NEL 在 Apple 规则下是空白
    assert_eq!(apple_name_error("\u{85}"), Some("请输入设备名称"));
    // NEL 在 Android 规则下是控制字符
    assert_eq!(
        android_name_error("\u{85}"),
        Some("设备名称不能包含控制字符")
    );
    // 0x1c 在 Android 规则下是空白
    assert_eq!(android_name_error("\u{1c}"), Some("请输入设备名称"));
    // 零宽空格在 Apple 规则下是空白
    assert_eq!(apple_name_error("\u{200b}"), Some("请输入设备名称"));
    assert!(name_error("\u{200b}").is_none());
}

// 名称长度按 UTF-8 字节计，不改动已保存的文本
#[test]
fn names_are_limited_by_utf8_bytes_without_changing_saved_text() {
    assert!(name_error(&"界".repeat(53)).is_none());
    assert!(name_error(&"界".repeat(54)).is_some());
    // 默认名称去掉首尾空白
    assert_eq!(android_default_name("  客厅平板  "), "客厅平板");
    // 型号非法则回落到默认名称
    assert_eq!(android_default_name("\u{85}"), "Android 显示端");
}

// 设备 ID 沿用规范小写、非零的 UUID 契约
#[test]
fn uuid_validation_preserves_canonical_nonzero_contract() {
    // 规范 UUID 合法
    assert!(valid_device_id("11111111-1111-1111-1111-111111111111"));
    // 全零非法
    assert!(!valid_device_id("00000000-0000-0000-0000-000000000000"));
    // 大写非法
    assert!(!valid_device_id("AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA"));
    // 路径样式的字符串非法
    assert!(!valid_device_id("../../identity"));
}
