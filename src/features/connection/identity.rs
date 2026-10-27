//! 身份与名称的唯一应用规则；UTF-8 字节上限不能用字符数量替代。
use serde_json::{Value, json};

pub(crate) const MAX_NAME_BYTES: usize = 160;
pub(crate) const ZERO_DEVICE_ID: &str = "00000000-0000-0000-0000-000000000000";

pub(crate) fn name_error(name: &str) -> Option<&'static str> {
    crate::shared::domain::identity::application_name_error(name)
}

pub(crate) fn valid_device_id(value: &str) -> bool {
    crate::shared::domain::identity::valid_id(value)
}

pub(crate) fn commit_device_name(name: &str, device_id: &str) -> Value {
    if let Some(error) = name_error(name) {
        json!({"ok": false, "error": error})
    } else if !valid_device_id(device_id) {
        json!({"ok": false, "error": "保存失败，请重试"})
    } else {
        json!({"ok": true, "name": name})
    }
}

#[cfg(test)]
#[path = "identity_test.rs"]
mod identity_test;
