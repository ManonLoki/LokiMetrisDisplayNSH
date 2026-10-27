use super::*;

#[test]
fn aliases_and_canonical_unicode_keep_one_identity() {
    assert_eq!(canonical_agent_key(" Claude_Code\u{feff}"), "claude");
    assert_eq!(canonical_agent_key("GitHub-Copilot CLI"), "githubcopilot");
    assert_eq!(canonical_agent_key("Grok Build"), "grok");
    assert_eq!(
        canonical_agent_key("CAFE\u{301}"),
        canonical_agent_key("Café")
    );
    assert_eq!(normalize_comparison_text("e\u{301}"), "é");
}

#[test]
fn unknown_agent_display_name_is_not_rewritten() {
    assert_eq!(agent_display_name("my unknown agent"), "my unknown agent");
    assert_eq!(agent_display_name("Claude_Code"), "Claude Code");
    assert_eq!(agent_display_name("gemini-cli"), "Gemini CLI");
}

/// The standalone Watch app checks the same fixture against its Swift table.
#[test]
fn display_names_match_the_shared_watch_fixture() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/display-names.json")).unwrap();
    for case in fixture["agents"].as_array().unwrap() {
        let raw = case[0].as_str().unwrap();
        assert_eq!(agent_display_name(raw), case[1], "{raw}");
    }
}
