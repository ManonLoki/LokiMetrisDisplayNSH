use super::*;

fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn aliases_match_enabled_agents_and_provider_quotas() {
    let enabled = names(&["claude", "grok"]);
    assert!(is_enabled(" Claude_Code ", Some(&enabled)));
    assert!(is_enabled("Grok-Build", Some(&enabled)));
    assert!(!is_enabled("Codex", Some(&enabled)));
    assert_eq!(provider_quota_key("Claude Code").as_deref(), Some("claude"));
    assert_eq!(provider_quota_key("Other Agent"), None);
}

#[test]
fn stable_agent_order_merges_provider_rows_without_mutating_sources() {
    let tokens = names(&[
        "Other B",
        "WorkBuddy",
        "Grok Build",
        "Claude Code",
        "codex",
        "Other A",
    ]);
    let quotas = names(&["claude", "grok"]);
    let rows = project_rows(true, &tokens, &quotas, None);
    assert_eq!(
        rows.iter().map(|row| row.token_index).collect::<Vec<_>>(),
        vec![4, 3, -1, 2, 0, 5, 1]
    );
    assert_eq!(rows[1].quota_index, 0);
    assert_eq!(rows[3].quota_index, 1);
    assert_eq!(tokens[0], "Other B");
}

#[test]
fn cursor_default_is_only_projected_when_enabled_and_context_exists() {
    assert!(project_rows(false, &[], &[], None).is_empty());
    let cursor = names(&["cursor"]);
    assert_eq!(project_rows(false, &[], &[], Some(&cursor)).len(), 1);
    assert!(project_rows(true, &[], &[], Some(&[])).is_empty());
    assert_eq!(
        token_text("Cursor", Some("无记录"), false).as_deref(),
        Some("0")
    );
    assert_eq!(token_text("Codex", None, false), None);
    assert_eq!(
        token_text("Cursor", Some("123"), true).as_deref(),
        Some("123")
    );
}
