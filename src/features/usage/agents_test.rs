// 引入被测模块
use super::*;

// 字符串切片转 String 列表
fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

// 别名同时匹配启用列表与供应商额度
#[test]
fn aliases_match_enabled_agents_and_provider_quotas() {
    let enabled = names(&["claude", "grok"]);
    // 大小写、空白与下划线不影响匹配
    assert!(is_enabled(" Claude_Code ", Some(&enabled)));
    assert!(is_enabled("Grok-Build", Some(&enabled)));
    // 不在列表中的未启用
    assert!(!is_enabled("Codex", Some(&enabled)));
    assert_eq!(provider_quota_key("Claude Code").as_deref(), Some("claude"));
    // 没有额度的代理
    assert_eq!(provider_quota_key("Other Agent"), None);
}

// 稳定的代理顺序，合并供应商行，且不修改来源
#[test]
fn stable_agent_order_merges_provider_rows_without_mutating_sources() {
    let tokens = names(&[
        "Other B",
        "WorkBuddy",
        "Grok Build",
        "Claude Code",
        "codex",
        // 名称样例
        "Other A",
    ]);
    // 额度供应商名单
    let quotas = names(&["claude", "grok"]);
    // 计算投影行
    let rows = project_rows(true, &tokens, &quotas, None);
    // 顺序：Codex、Claude、Cursor 默认行、Grok、其他、WorkBuddy 在末尾；纯额度行在原位
    assert_eq!(
        rows.iter().map(|row| row.token_index).collect::<Vec<_>>(),
        vec![4, 3, -1, 2, 0, 5, 1]
    );
    // Claude 行匹配到 claude 额度
    assert_eq!(rows[1].quota_index, 0);
    // Grok 行匹配到 grok 额度
    assert_eq!(rows[3].quota_index, 1);
    // 来源列表未被改动
    assert_eq!(tokens[0], "Other B");
}

// Cursor 默认行只在启用且有上下文时出现
#[test]
fn cursor_default_is_only_projected_when_enabled_and_context_exists() {
    // 没有数据也没有启用列表：不显示
    assert!(project_rows(false, &[], &[], None).is_empty());
    let cursor = names(&["cursor"]);
    // 启用了 Cursor：显示一行
    assert_eq!(project_rows(false, &[], &[], Some(&cursor)).len(), 1);
    // 启用列表为空：不显示
    assert!(project_rows(true, &[], &[], Some(&[])).is_empty());
    // Cursor 无数值时显示 0
    assert_eq!(
        token_text("Cursor", Some("无记录"), false).as_deref(),
        Some("0")
    );
    // 其他代理没有文本就没有
    assert_eq!(token_text("Codex", None, false), None);
    // Cursor 有数值时用真实值
    assert_eq!(
        token_text("Cursor", Some("123"), true).as_deref(),
        Some("123")
    );
}
