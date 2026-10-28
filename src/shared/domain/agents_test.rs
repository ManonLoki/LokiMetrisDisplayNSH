// 引入被测模块
use super::*;

#[test]
// 别名与规范 Unicode 写法都归一为同一个身份
fn aliases_and_canonical_unicode_keep_one_identity() {
    assert_eq!(canonical_agent_key(" Claude_Code\u{feff}"), "claude");
    assert_eq!(canonical_agent_key("GitHub-Copilot CLI"), "githubcopilot");
    assert_eq!(canonical_agent_key("Grok Build"), "grok");
    assert_eq!(
        canonical_agent_key("CAFE\u{301}"),
        canonical_agent_key("Café")
    );
    // 带组合重音的字符规范化为单个字符
    assert_eq!(normalize_comparison_text("e\u{301}"), "é");
}

#[test]
// 未知代理的显示名保持原样
fn unknown_agent_display_name_is_not_rewritten() {
    assert_eq!(agent_display_name("my unknown agent"), "my unknown agent");
    assert_eq!(agent_display_name("Claude_Code"), "Claude Code");
    assert_eq!(agent_display_name("gemini-cli"), "Gemini CLI");
}

/// 独立的手表应用会用同一份夹具核对其 Swift 表。
#[test]
// 显示名与手表应用共用的夹具一致
fn display_names_match_the_shared_watch_fixture() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/display-names.json")).unwrap();
    for case in fixture["agents"].as_array().unwrap() {
        let raw = case[0].as_str().unwrap();
        assert_eq!(agent_display_name(raw), case[1], "{raw}");
    }
}
