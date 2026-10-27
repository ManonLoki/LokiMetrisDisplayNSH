use super::*;
use crate::summary::{
    from_message,
    summary_test::{SOURCE, message},
};

#[test]
fn view_formats_totals_quota_hooks_and_rows() {
    let record = from_message(&message(), SOURCE, 1).unwrap();
    let view = project(&record["snapshot"]);
    assert_eq!(view["tokens"]["total"]["compact"], "18446744073.71B");
    assert_eq!(
        view["tokens"]["total"]["exact"],
        "18,446,744,073,709,551,615"
    );
    assert_eq!(view["tokens"]["total"]["quality"], "部分·估算");
    assert_eq!(view["tokens"]["total"]["compactWide"], "18.45E");
    assert_eq!(
        view["behaviorCounts"][0],
        json!({"behavior": "running", "label": "运行", "count": 1, "text": "运行 1"})
    );
    assert_eq!(view["hooksText"], "Hooks 1");
    assert_eq!(view["hooks"][0]["label"], "运行");
    let provider = &view["quota"]["providers"][0];
    assert_eq!(provider["displayName"], "Codex，套餐 Plus");
    assert_eq!(provider["windows"][0]["percent"], "75%");
    assert_eq!(provider["windows"][0]["level"], 2);
    // 旧桌面没有启用名单：不新增 Cursor 占位。
    let rows = view["agents"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], "Codex");
    assert_eq!(rows[0]["compact"], "1.5K");
}

#[test]
fn enabled_list_filters_quota_and_adds_cursor_default() {
    let mut message = message();
    message["snapshot"]["enabledAgents"] = json!(["Cursor", "claude"]);
    let view = project(&from_message(&message, SOURCE, 1).unwrap()["snapshot"]);
    assert!(view["quota"].is_null(), "codex quota is disabled");
    assert_eq!(
        view["agents"],
        json!([{"id": "default:cursor", "name": "Cursor", "plan": null,
        "text": "0", "compact": "0", "exact": null, "quality": "", "accessibilityLabel": "Cursor，0"}])
    );
}

#[test]
fn missing_data_uses_disclosed_placeholders() {
    let view = project(
        &json!({"version": 1, "tokens": null, "tokenError": "x", "hooksAvailable": false,
        "hooks": [], "providerQuota": null, "enabledAgents": null}),
    );
    assert_eq!(view["tokenFallback"], "不可用");
    assert_eq!(view["agentsEmptyText"], "暂不可用");
    assert_eq!(view["hooksText"], "Hooks 未同步");
}

#[test]
fn desktop_camel_case_names_match_tokens_and_quota_aliases() {
    let mut message = message();
    message["snapshot"]["enabledAgents"] = json!(["codex", "claudeCode", "gitHubCopilot"]);
    message["snapshot"]["tokens"]["agents"] = json!([
        {"agent": "claude-code", "amount": {"total": "7", "availability": "available", "completeness": "complete", "confidence": "exact"}},
        {"agent": "GitHub Copilot CLI", "amount": {"total": null, "availability": "noRecords", "completeness": "complete", "confidence": "exact"}},
        {"agent": "grok-build", "amount": {"total": "9", "availability": "available", "completeness": "complete", "confidence": "exact"}},
    ]);
    let view = project(&from_message(&message, SOURCE, 1).unwrap()["snapshot"]);
    let names: Vec<_> = view["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    // Codex 只有额度，排在最前；未启用的 Grok 不显示；无记录保持文字而非零。
    assert_eq!(names, ["Codex", "Claude Code", "GitHub Copilot"]);
    assert_eq!(view["agents"][2]["text"], "无记录");
    assert_eq!(view["quota"]["providers"][0]["provider"], "codex");
}

#[test]
fn explicit_empty_list_hides_every_row_without_changing_totals() {
    let mut message = message();
    message["snapshot"]["enabledAgents"] = json!([]);
    let view = project(&from_message(&message, SOURCE, 1).unwrap()["snapshot"]);
    assert_eq!(view["agents"], json!([]));
    assert!(view["quota"].is_null());
    assert_eq!(
        view["tokens"]["total"]["exact"],
        "18,446,744,073,709,551,615"
    );
}
