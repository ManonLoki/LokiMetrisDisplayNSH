use super::*;
use serde_json::json;

#[test]
fn integer_rounding_promotes_units_without_losing_full_u64_precision() {
    for (raw, expected) in [
        ("999", "999"),
        ("1500", "1.5K"),
        ("1005", "1.01K"),
        ("999994", "999.99K"),
        ("999995", "1M"),
        ("221193571", "221.19M"),
        ("1000000000000", "1000B"),
        ("18446744073709551615", "18446744073.71B"),
    ] {
        assert_eq!(token_compact(raw), expected, "{raw}");
    }
    assert_eq!(
        token_exact("18446744073709551615"),
        "18,446,744,073,709,551,615"
    );
}

#[test]
fn unavailable_or_invalid_decimals_do_not_become_numeric_zero() {
    for raw in [
        "不可用",
        "",
        "-1",
        "1.5",
        "18446744073709551616",
        "１２３４",
    ] {
        assert_eq!(token_compact(raw), raw);
    }
    assert_eq!(token_exact("1.5"), "1.5");
    assert_eq!(token_compact("000999"), "000999");
    assert_eq!(token_exact("000999"), "000,999");
    assert_eq!(amount_label(&json!({"availability":"noRecords"})), "无记录");
    assert_eq!(
        amount_label(&json!({"availability":"unavailable"})),
        "不可用"
    );
    assert!(!has_numeric_value(
        &json!({"availability":"available","total":null})
    ));
}

#[test]
fn accessibility_preserves_completeness_and_estimation_qualifiers() {
    let amount = json!({"availability":"available","total":"18446744073709551615",
        "completeness":"partial","confidence":"derived"});
    assert_eq!(
        amount_accessibility(&amount),
        "Token 18446744073709551615，部分统计，推算"
    );
    assert_eq!(
        amount_accessibility(&json!({"availability":"available","total":"0",
        "completeness":"complete","confidence":"exact"})),
        "Token 0"
    );
}
