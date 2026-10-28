// 引入 JSON 宏
use serde_json::json;

// 非法参数被拒绝，但不会抢占其他功能的操作
#[test]
fn dispatch_rejects_invalid_arguments_without_claiming_other_features() {
    let dispatch = super::dispatch;
    // 别的功能的操作返回 None
    assert!(dispatch("hooks.label", &[]).is_none());
    // 本功能下未知的操作也返回 None
    assert!(dispatch("usage.unknown", &[]).is_none());
    // 缺少参数报错
    assert!(dispatch("usage.cardProjection", &[]).unwrap().is_err());
    // 参数为 null 时得到空投影
    let card = dispatch("usage.cardProjection", &[json!(null)])
        .unwrap()
        .unwrap();
    assert_eq!(card["hasTokens"], false);
}
