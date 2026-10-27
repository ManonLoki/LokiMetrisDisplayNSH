/// The standalone Watch app checks the same fixture against its Swift labels.
#[test]
fn status_labels_match_the_shared_watch_fixture() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/display-names.json")).unwrap();
    for case in fixture["behaviors"].as_array().unwrap() {
        let label = super::dispatch("hooks.label", std::slice::from_ref(&case[0]))
            .unwrap()
            .unwrap();
        assert_eq!(label, case[1], "{}", case[0]);
    }
}
