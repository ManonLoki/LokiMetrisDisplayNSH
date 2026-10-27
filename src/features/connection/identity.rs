//! 设备身份与名称的唯一规则，含旧版移动端恢复名称的空白语义；UTF-8 字节上限不能用字符数量替代。

pub(crate) const MAX_NAME_BYTES: usize = 160;
pub(crate) const DEFAULT_NAME: &str = "LokiMetris Display";

/// Canonical lowercase, nonzero UUIDs; the pairing core owns this rule for both peers.
pub(crate) use loki_metris_pairing::valid_device_id;

fn issue(value: &str, blank: bool) -> Option<&'static str> {
    if blank {
        Some("请输入设备名称")
    } else if value
        .chars()
        .any(|ch| ch <= '\u{1f}' || ('\u{7f}'..='\u{9f}').contains(&ch))
    {
        Some("设备名称不能包含控制字符")
    } else if value.len() > MAX_NAME_BYTES {
        Some("设备名称不能超过 160 个 UTF-8 字节")
    } else {
        None
    }
}

/// Validates names entered through the shared NSH application.
pub(crate) fn name_error(value: &str) -> Option<&'static str> {
    issue(
        value,
        value
            .trim_matches(crate::shared::domain::text::comparison_whitespace)
            .is_empty(),
    )
}

#[cfg(any(target_os = "android", test))]
fn kotlin_whitespace(ch: char) -> bool {
    ch.is_whitespace() && ch != '\u{85}' || ('\u{1c}'..='\u{1f}').contains(&ch)
}

/// Preserves Kotlin/JVM blank-name behavior for saved Android names.
#[cfg(any(target_os = "android", test))]
pub(crate) fn android_name_error(value: &str) -> Option<&'static str> {
    issue(value, value.trim_matches(kotlin_whitespace).is_empty())
}

/// Uses the device model for a new Android installation when it is a valid name.
#[cfg(any(target_os = "android", test))]
pub(crate) fn android_default_name(model: &str) -> String {
    let model = model.trim_matches(kotlin_whitespace);
    if android_name_error(model).is_none() {
        model.into()
    } else {
        "Android 显示端".into()
    }
}

/// Preserves Foundation whitespace and zero-width-space behavior for saved Apple names.
#[cfg(any(target_os = "ios", test))]
pub(crate) fn apple_name_error(value: &str) -> Option<&'static str> {
    issue(
        value,
        value
            .chars()
            .all(|ch| ch.is_whitespace() || ch == '\u{200b}'),
    )
}

#[cfg(test)]
#[path = "identity_test.rs"]
mod identity_test;
