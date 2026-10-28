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

// 分页：1x1 每页一个，2x2 每页四个，没有 Hook 时仍有一页空格
#[test]
fn pages_fill_cells_in_order_and_keep_one_empty_page() {
    assert_eq!(super::pages(0, "1x1"), vec![vec![None]]);
    assert_eq!(super::pages(0, "2x2"), vec![vec![None; 4]]);
    assert_eq!(super::pages(2, "1x1"), vec![vec![Some(0)], vec![Some(1)]]);
    assert_eq!(
        super::pages(5, "2x2"),
        vec![vec![Some(0), Some(1), Some(2), Some(3)], vec![Some(4), None, None, None]]
    );
}
