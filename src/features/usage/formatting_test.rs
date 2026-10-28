// 引入被测模块
use super::*;
use serde_json::json;

// 整数四舍五入时单位会进位，且不丢失完整 u64 的精度
#[test]
fn integer_rounding_promotes_units_without_losing_full_u64_precision() {
    // 覆盖边界：不足千、进位到下一档、最大值
    for (raw, expected) in [
        ("999", "999"),
        ("1500", "1.5K"),
        ("1005", "1.01K"),
        ("999994", "999.99K"),
        ("999995", "1M"),
        ("221193571", "221.19M"),
        ("1000000000000", "1000B"),
        // u64 最大值的紧凑写法
        ("18446744073709551615", "18446744073.71B"),
        // 逐项检查
    ] {
        // 紧凑写法符合预期
        assert_eq!(token_compact(raw), expected, "{raw}");
    }
    // 精确写法每三位加逗号
    assert_eq!(
        token_exact("18446744073709551615"),
        "18,446,744,073,709,551,615"
    );
}

// 不可用或非法的十进制不会变成数值 0
#[test]
fn unavailable_or_invalid_decimals_do_not_become_numeric_zero() {
    // 非数字、空串、负数、小数、超出 u64、全角数字，都原样返回
    for raw in [
        "不可用",
        "",
        "-1",
        "1.5",
        "18446744073709551616",
        "１２３４",
    ] {
        // 无需压缩的值保持原样
        assert_eq!(token_compact(raw), raw);
    }
    // 精确写法保持原样
    assert_eq!(token_exact("1.5"), "1.5");
    // 前导零的纯数字不足一千，原样返回
    assert_eq!(token_compact("000999"), "000999");
    assert_eq!(token_exact("000999"), "000,999");
    assert_eq!(amount_label(&json!({"availability":"noRecords"})), "无记录");
    assert_eq!(
        amount_label(&json!({"availability":"unavailable"})),
        "不可用"
    );
    assert!(!has_numeric_value(
        // 可用但没有总量
        &json!({"availability":"available","total":null})
    ));
}

// 无障碍文案保留完整度与估算的限定词
#[test]
fn accessibility_preserves_completeness_and_estimation_qualifiers() {
    // 部分统计、推算
    let amount = json!({"availability":"available","total":"18446744073709551615",
        "completeness":"partial","confidence":"derived"});
    assert_eq!(
        amount_accessibility(&amount),
        "Token 18446744073709551615，部分统计，推算"
    );
    // 完整且精确时没有限定词
    assert_eq!(
        amount_accessibility(&json!({"availability":"available","total":"0",
        "completeness":"complete","confidence":"exact"})),
        "Token 0"
    );
}
