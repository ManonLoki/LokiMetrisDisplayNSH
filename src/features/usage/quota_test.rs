use super::*;

#[test]
fn provider_projection_keeps_only_supplied_windows_in_display_order() {
    let provider = json!({"provider":"grok","plan":"pro","weekly":null,
        "fiveHour":{"kind":"fiveHour","usedTenths":801},
        "monthly":{"kind":"monthly","usedTenths":499}});
    let projection = provider_projection(&provider);
    assert_eq!(projection["name"], "Grok");
    assert_eq!(projection["plan"], "Pro");
    assert_eq!(projection["windows"].as_array().unwrap().len(), 2);
    assert_eq!(
        projection["windows"][0],
        json!({"kindLabel":"每月","percent":"50.1%",
        "level":"green","remainingTenths":501})
    );
    assert_eq!(projection["windows"][1]["percent"], "19.9%");
    assert!(plan_label(Some("")).is_none());
}

#[test]
fn balance_warning_thresholds_include_exact_boundary_values() {
    for (used, level) in [
        (800, "red"),
        (900, "red"),
        (799, "yellow"),
        (500, "yellow"),
        (499, "green"),
    ] {
        assert_eq!(quota_level(used), level);
    }
    assert_eq!(percent_text(0), "100%");
    assert_eq!(percent_text(1000), "0%");
    assert_eq!(percent_text(795), "20.5%");
}
