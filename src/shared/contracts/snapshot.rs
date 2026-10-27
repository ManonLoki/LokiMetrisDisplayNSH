//! 所有接收入口共同使用的基本约束；原始网络帧另校验资源描述与时间区间。
use crate::shared::domain::agents::canonical_agent_key;
use serde_json::Value;

pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub fn safe_unsigned(value: &Value) -> Option<u64> {
    if let Some(number) = value.as_u64() {
        return (number <= MAX_SAFE_INTEGER).then_some(number);
    }
    value.as_f64().and_then(|number| {
        (number.is_finite()
            && number >= 0.0
            && number.fract() == 0.0
            && number <= MAX_SAFE_INTEGER as f64)
            .then_some(number as u64)
    })
}

pub fn valid_agent_name(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        !text.is_empty() && text.len() <= 64 && !canonical_agent_key(text).is_empty()
    })
}

pub fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn valid_amount(value: &Value) -> bool {
    if !value.is_object()
        || !value["availability"]
            .as_str()
            .is_some_and(|text| ["available", "noRecords", "unavailable"].contains(&text))
        || !value["completeness"]
            .as_str()
            .is_some_and(|text| ["complete", "partial", "unknown"].contains(&text))
        || !value["confidence"]
            .as_str()
            .is_some_and(|text| ["exact", "derived", "suspected"].contains(&text))
    {
        return false;
    }
    if value["availability"] != "available" {
        return value["total"].is_null();
    }
    value["total"].as_str().is_some_and(|text| {
        !text.is_empty()
            && text.len() <= 20
            && (text == "0" || !text.starts_with('0'))
            && text.bytes().all(|byte| byte.is_ascii_digit())
            && text.parse::<u64>().is_ok()
    })
}

pub fn valid_date(value: &str) -> bool {
    value.len() == 10
        && value.bytes().enumerate().all(|(index, byte)| {
            if index == 4 || index == 7 {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
}
