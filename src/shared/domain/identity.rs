//! Device identity and name contracts, including names restored from older mobile releases.

/// Accepts canonical lowercase, nonzero UUIDs without imposing a new version requirement.
pub fn valid_id(value: &str) -> bool {
    value.len() == 36
        && value != "00000000-0000-0000-0000-000000000000"
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
        })
}

fn issue(value: &str, blank: bool) -> Option<&'static str> {
    if blank {
        Some("请输入设备名称")
    } else if value
        .chars()
        .any(|ch| ch <= '\u{1f}' || ('\u{7f}'..='\u{9f}').contains(&ch))
    {
        Some("设备名称不能包含控制字符")
    } else if value.len() > 160 {
        Some("设备名称不能超过 160 个 UTF-8 字节")
    } else {
        None
    }
}

/// Validates names entered through the shared NSH application.
pub fn application_name_error(value: &str) -> Option<&'static str> {
    issue(
        value,
        value
            .trim_matches(crate::shared::domain::agents::comparison_whitespace)
            .is_empty(),
    )
}

#[cfg(any(target_os = "android", test))]
fn kotlin_whitespace(ch: char) -> bool {
    ch.is_whitespace() && ch != '\u{85}' || ('\u{1c}'..='\u{1f}').contains(&ch)
}

/// Preserves Kotlin/JVM blank-name behavior for saved Android names.
#[cfg(any(target_os = "android", test))]
pub fn android_name_error(value: &str) -> Option<&'static str> {
    issue(value, value.trim_matches(kotlin_whitespace).is_empty())
}

/// Uses the device model for a new Android installation when it is a valid name.
#[cfg(any(target_os = "android", test))]
pub fn android_default_name(model: &str) -> String {
    let model = model.trim_matches(kotlin_whitespace);
    if android_name_error(model).is_none() {
        model.into()
    } else {
        "Android 显示端".into()
    }
}

/// Preserves Foundation whitespace and zero-width-space behavior for saved Apple names.
#[cfg(any(target_os = "ios", test))]
pub fn apple_name_error(value: &str) -> Option<&'static str> {
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
