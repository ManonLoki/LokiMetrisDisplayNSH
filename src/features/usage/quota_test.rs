// 引入被测模块
use super::*;

// 供应商投影只保留服务端提供的窗口，并按展示顺序排列
#[test]
fn provider_projection_keeps_only_supplied_windows_in_display_order() {
    // 周窗口为 null，只有五小时与月
    let provider = json!({"provider":"grok","plan":"pro","weekly":null,
        "fiveHour":{"kind":"fiveHour","usedTenths":801},
        "monthly":{"kind":"monthly","usedTenths":499}});
    let projection = provider_projection(&provider);
    assert_eq!(projection["name"], "Grok");
    // 套餐首字母大写
    assert_eq!(projection["plan"], "Pro");
    // 缺失的窗口不补
    assert_eq!(projection["windows"].as_array().unwrap().len(), 2);
    // 月窗口排在最前
    assert_eq!(
        projection["windows"][0],
        json!({"kindLabel":"每月","percent":"50.1%",
        "level":"green","remainingTenths":501})
    );
    assert_eq!(projection["windows"][1]["percent"], "19.9%");
    // 空套餐无标签
    assert!(plan_label(Some("")).is_none());
}

// 余额警告阈值包含边界值
#[test]
fn balance_warning_thresholds_include_exact_boundary_values() {
    // 剩余 20% 及以下红，剩余 50% 及以下黄，其余绿
    for (used, level) in [
        (800, "red"),
        (900, "red"),
        (799, "yellow"),
        (500, "yellow"),
        (499, "green"),
    ] {
        assert_eq!(quota_level(used), level);
    }
    // 剩余全部时显示 100%
    assert_eq!(percent_text(0), "100%");
    // 全部用完显示 0%
    assert_eq!(percent_text(1000), "0%");
    // 带小数的百分比
    assert_eq!(percent_text(795), "20.5%");
}
