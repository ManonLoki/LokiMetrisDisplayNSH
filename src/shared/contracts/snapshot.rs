//! 所有接收入口共同使用的基本约束；原始网络帧另校验资源描述与时间区间。
// 引入规范代理键
use crate::shared::domain::agents::canonical_agent_key;
// 引入 JSON 值
use serde_json::Value;

// JavaScript 能精确表示的最大整数（2^53 - 1）
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

// 读取安全的非负整数，整数值的浮点数也接受
pub fn safe_unsigned(value: &Value) -> Option<u64> {
    // 整数
    if let Some(number) = value.as_u64() {
        // 不超过安全整数才有效
        return (number <= MAX_SAFE_INTEGER).then_some(number);
    }
    // 浮点数须有限、非负、无小数且不超过上限
    value.as_f64().and_then(|number| {
        // 有限数值
        (number.is_finite()
            // 非负
            && number >= 0.0
            // 无小数部分
            && number.fract() == 0.0
            // 不超过安全整数
            && number <= MAX_SAFE_INTEGER as f64)
            // 转为无符号整数
            .then_some(number as u64)
    })
}

// 代理名非空、不超过 64 字节且归一后不为空
pub fn valid_agent_name(value: &Value) -> bool {
    // 字符串
    value.as_str().is_some_and(|text| {
        // 非空、长度不超过 64 且有规范键
        !text.is_empty() && text.len() <= 64 && !canonical_agent_key(text).is_empty()
    })
}

// 哈希为 64 位小写十六进制
pub fn valid_hash(value: &str) -> bool {
    // 长度为 64
    value.len() == 64
        // 并且
        && value
            // 按字节
            .bytes()
            // 都是小写十六进制
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

// 用量必须是对象，三个状态字段取值合法
pub fn valid_amount(value: &Value) -> bool {
    // 不是对象
    if !value.is_object()
        // 或者可用性不对
        || !value["availability"]
            // 取字符串
            .as_str()
            // 须是三种取值之一
            .is_some_and(|text| ["available", "noRecords", "unavailable"].contains(&text))
        // 或者完整性不对
        || !value["completeness"]
            // 取字符串
            .as_str()
            // 须是三种取值之一
            .is_some_and(|text| ["complete", "partial", "unknown"].contains(&text))
        // 置信度
        || !value["confidence"]
            // 取字符串
            .as_str()
            // 须为三种取值之一
            .is_some_and(|text| ["exact", "derived", "suspected"].contains(&text))
    {
        // 不合格
        return false;
    }
    // 不可用时不能有数值
    if value["availability"] != "available" {
        // 总量必须为空
        return value["total"].is_null();
    }
    // 可用时数值须为不超过 20 位、无前导零的十进制且能放进 u64
    value["total"].as_str().is_some_and(|text| {
        // 非空
        !text.is_empty()
            // 长度不超过 20
            && text.len() <= 20
            // “0”或不以 0 开头
            && (text == "0" || !text.starts_with('0'))
            // 全为数字
            && text.bytes().all(|byte| byte.is_ascii_digit())
            // 可解析为 u64
            && text.parse::<u64>().is_ok()
    })
}

// 日期形如 YYYY-MM-DD
pub fn valid_date(value: &str) -> bool {
    // 长度为 10
    value.len() == 10
        // 按字节带下标检查
        && value.bytes().enumerate().all(|(index, byte)| {
            // 第 5 位与第 8 位
            if index == 4 || index == 7 {
                // 须为连字符
                byte == b'-'
            // 否则分支
            } else {
                // 其余须为数字
                byte.is_ascii_digit()
            }
        })
}
