//! 名称与 Agent 共用的比较文本规则：NFC 归一化和 ECMAScript 空白定义。
// 引入 ICU 的 NFC 归一化器
use icu_normalizer::ComposingNormalizer;

// 把文本归一为 NFC，使不同组合方式的同一字符相等
pub fn normalize_comparison_text(value: &str) -> String {
    // NFC 规范化
    ComposingNormalizer::new_nfc().normalize(value).into_owned()
}

/// ECMAScript 的空白定义也用于旧偏好/设备名称，避免迁移改变既有身份。
// ECMAScript 的空白定义，旧偏好与设备名称沿用以免迁移改变身份
pub fn comparison_whitespace(ch: char) -> bool {
    // 各种空白字符
    matches!(ch, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}'
        // 各种空白字符
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        // 分隔符与字节序标记
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
