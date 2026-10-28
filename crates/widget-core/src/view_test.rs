// 引入被测展示投影
use super::*;
use crate::summary::{
    from_message,
    summary_test::{SOURCE, message},
};

#[test]
// 展示投影格式化总量、额度、Hooks 与明细行
fn view_formats_totals_quota_hooks_and_rows() {
    // 由测试消息生成摘要
    let record = from_message(&message(), SOURCE, 1).unwrap();
    // 投影为展示模型
    let view = project(&record["snapshot"]);
    // 总量的紧凑、精确与无障碍写法
    assert_eq!(view["tokens"]["total"]["compact"], "18446744073.71B");
    assert_eq!(
        view["tokens"]["total"]["exact"],
        "18,446,744,073,709,551,615"
    );
    assert_eq!(
        // 无障碍文案带“部分·估算”限定
        view["tokens"]["total"]["accessibilityLabel"],
        "总 Token，18446744073709551615，部分·估算"
    );
    // Hooks 文案与数量
    assert_eq!(view["hooksText"], "Hooks 1");
    assert_eq!(view["hookCountText"], "1");
    // 额度：名称、套餐与描述
    let provider = &view["quota"]["providers"][0];
    assert!(
        provider["description"]
            .as_str()
            .unwrap()
            .starts_with("Codex，套餐 Plus，")
    );
    // 已用 25%，剩余 75%，等级为绿（2）
    assert_eq!(provider["windows"][0]["percent"], "75%");
    assert_eq!(provider["windows"][0]["level"], 2);
    // 旧桌面没有启用名单：不新增 Cursor 占位。
    // 旧桌面没有启用名单：不新增 Cursor 占位
    let rows = view["agents"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], "Codex");
    assert_eq!(rows[0]["compact"], "1.5K");
}

#[test]
// 启用列表过滤额度，并补上 Cursor 默认行
fn enabled_list_filters_quota_and_adds_cursor_default() {
    // 启用列表只含 Cursor 和 Claude
    let mut message = message();
    message["snapshot"]["enabledAgents"] = json!(["Cursor", "claude"]);
    let view = project(&from_message(&message, SOURCE, 1).unwrap()["snapshot"]);
    // Codex 的额度被过滤掉
    assert!(view["quota"].is_null(), "codex quota is disabled");
    // 明细行里只剩 Cursor 的默认行（显示 0）
    assert_eq!(
        view["agents"],
        json!([{"id": "default:cursor", "name": "Cursor", "plan": null,
        "text": "0", "compact": "0", "exact": null, "accessibilityLabel": "Cursor，0"}])
    );
}

#[test]
// 数据缺失时使用明示的占位文案
fn missing_data_uses_disclosed_placeholders() {
    // 没有 Token、只有错误且 Hooks 不可用
    let view = project(
        &json!({"version": 1, "tokens": null, "tokenError": "x", "hooksAvailable": false,
        "hooks": [], "providerQuota": null, "enabledAgents": null}),
    );
    // 占位文案明示数据不可用
    assert_eq!(view["tokenFallback"], "不可用");
    assert_eq!(view["agentsEmptyText"], "暂不可用");
    assert_eq!(view["hooksText"], "Hooks 未同步");
    assert_eq!(view["hookCountText"], "—");
    assert!(view["hookLead"].is_null());
}

#[test]
// 桌面端的驼峰命名能匹配 Token 与额度的别名
fn desktop_camel_case_names_match_tokens_and_quota_aliases() {
    // 桌面端驼峰命名（claudeCode 等）与带连字符的名称应互相匹配
    let mut message = message();
    message["snapshot"]["enabledAgents"] = json!(["codex", "claudeCode", "gitHubCopilot"]);
    // 三个代理：claude-code、GitHub Copilot CLI（无记录）、grok-build（未启用）
    message["snapshot"]["tokens"]["agents"] = json!([
        {"agent": "claude-code", "amount": {"total": "7", "availability": "available", "completeness": "complete", "confidence": "exact"}},
        {"agent": "GitHub Copilot CLI", "amount": {"total": null, "availability": "noRecords", "completeness": "complete", "confidence": "exact"}},
        {"agent": "grok-build", "amount": {"total": "9", "availability": "available", "completeness": "complete", "confidence": "exact"}},
    ]);
    let view = project(&from_message(&message, SOURCE, 1).unwrap()["snapshot"]);
    let names: Vec<_> = view["agents"]
        .as_array()
        // 取出行数组
        .unwrap()
        // 遍历
        .iter()
        // 取每行的名称
        .map(|row| row["name"].as_str().unwrap())
        // 收集为列表
        .collect();
    // Codex 只有额度，排在最前；未启用的 Grok 不显示；无记录保持文字而非零。
    // Codex 只有额度，排最前；未启用的 Grok 不显示
    assert_eq!(names, ["Codex", "Claude Code", "GitHub Copilot"]);
    // 无记录保持文字而不是 0
    assert_eq!(view["agents"][2]["text"], "无记录");
    assert_eq!(view["quota"]["providers"][0]["provider"], "codex");
}

#[test]
// 显式的空启用列表隐藏所有行，但不改变总量
fn explicit_empty_list_hides_every_row_without_changing_totals() {
    let mut message = message();
    // 显式的空列表
    message["snapshot"]["enabledAgents"] = json!([]);
    let view = project(&from_message(&message, SOURCE, 1).unwrap()["snapshot"]);
    // 所有行与额度都被隐藏
    assert_eq!(view["agents"], json!([]));
    assert!(view["quota"].is_null());
    // 但总量不变
    assert_eq!(
        view["tokens"]["total"]["exact"],
        "18,446,744,073,709,551,615"
    );
}
