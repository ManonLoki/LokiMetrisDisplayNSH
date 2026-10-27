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
