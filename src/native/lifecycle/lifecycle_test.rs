#[test]
// 非桌面与无界面的平台保持“未知”
fn non_desktop_and_headless_platforms_remain_unknown() {
    for os in ["ios", "android", "web", "unknown", ""] {
        assert_eq!(super::foreground(os), None);
    }
}
