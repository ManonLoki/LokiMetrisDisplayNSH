/// 独立的手表应用会用同一份夹具核对其 Swift 标签。
// Hook 状态文案与手表应用共用的夹具一致
#[test]
fn status_labels_match_the_shared_watch_fixture() {
    // 读取共享夹具
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/display-names.json")).unwrap();
    // 逐个状态核对文案
    for case in fixture["behaviors"].as_array().unwrap() {
        let label = super::label(case[0].as_str().unwrap());
        assert_eq!(label, case[1], "{}", case[0]);
    }
}
