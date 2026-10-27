//! 名称与 Agent 共用的比较文本规则：NFC 归一化和 ECMAScript 空白定义。
use icu_normalizer::ComposingNormalizer;

pub fn normalize_comparison_text(value: &str) -> String {
    ComposingNormalizer::new_nfc().normalize(value).into_owned()
}

/// ECMAScript 的空白定义也用于旧偏好/设备名称，避免迁移改变既有身份。
pub fn comparison_whitespace(ch: char) -> bool {
    matches!(ch, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
