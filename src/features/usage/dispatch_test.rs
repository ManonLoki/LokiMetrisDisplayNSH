use serde_json::json;

#[test]
fn dispatch_rejects_invalid_arguments_without_claiming_other_features() {
    let dispatch = super::dispatch;
    assert!(dispatch("hooks.label", &[]).is_none());
    assert!(dispatch("usage.unknown", &[]).is_none());
    assert!(dispatch("usage.tokenCompact", &[]).unwrap().is_err());
    assert!(
        dispatch("usage.percentText", &[json!(1001)])
            .unwrap()
            .is_err()
    );
    assert_eq!(
        dispatch("usage.tokenCompact", &[json!("1005")])
            .unwrap()
            .unwrap(),
        json!("1.01K")
    );
}
