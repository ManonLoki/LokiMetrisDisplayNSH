use serde_json::json;

#[test]
fn dispatch_rejects_invalid_arguments_without_claiming_other_features() {
    let dispatch = super::dispatch;
    assert!(dispatch("hooks.label", &[]).is_none());
    assert!(dispatch("usage.unknown", &[]).is_none());
    assert!(dispatch("usage.cardProjection", &[]).unwrap().is_err());
    let card = dispatch("usage.cardProjection", &[json!(null)])
        .unwrap()
        .unwrap();
    assert_eq!(card["hasTokens"], false);
}
