// 引入被测模块
use super::*;

// 构造含多个代理、两家额度与启用列表的快照
fn snapshot() -> Value {
    let mut agents = Vec::new();
    // 其中 SecretBot 不在启用列表里
    for name in [
        "Other B",
        "WorkBuddy",
        "Grok Build",
        "Claude Code",
        "codex",
        "Other A",
        "SecretBot",
        // 逐个代理名
    ] {
        // 加入一条代理数据
        agents.push(
            // 总量 1500 可用
            json!({"agent":name,"amount":{"total":"1500","availability":"available",
            // 完整且精确
            "completeness":"complete","confidence":"exact"}}),
        );
    }
    // 快照：日期与时区
    json!({"tokens":{"day":{"date":"2026-10-26","zone":"Asia/Shanghai"},
        // 总量为 u64 最大值
        "all":{"total":"18446744073709551615","availability":"available",
            // 部分且为推算，附代理列表
            "completeness":"partial","confidence":"derived"},"agents":agents},
        // 启用名单
        "enabledAgents":["codex","claude","cursor","grok","workbuddy","Other B","Other A"],
        // 无 Token 错误，额度观测时间与供应商
        "tokenError":null,"providerQuota":{"observedAtUnix":1793019996,"providers":[
            // codex 的每周额度
            {"provider":"codex","plan":"plus","weekly":{"kind":"weekly","usedTenths":500}},
            // grok 的每月额度
            {"provider":"grok","plan":null,"monthly":{"kind":"monthly","usedTenths":800},
                // 与五小时额度
                "fiveHour":{"kind":"fiveHour","usedTenths":300}}]}})
}

// 展示投影合并别名并过滤未启用的代理
#[test]
fn display_projection_merges_aliases_and_filters_disabled_agents() {
    let input = snapshot();
    // 用于确认输入未被修改
    let before = input.clone();
    let rows = display_rows(&input);
    // 行的顺序与名称
    assert_eq!(
        rows.iter()
            .map(|row| row["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "Codex",
            "Claude Code",
            "Cursor",
            // 期望的第四个名称
            "Grok",
            // 其他代理按名称排序
            "Other B",
            // 其他代理按名称排序
            "Other A",
            // WorkBuddy 排在最后
            "WorkBuddy"
        ]
    );
    // Codex 的额度窗口
    assert_eq!(rows[0]["windows"][0]["kindLabel"], "每周");
    // Cursor 默认行显示 0
    assert_eq!(rows[2]["tokenText"], "0");
    // 且没有额度窗口
    assert_eq!(rows[2]["windows"], json!([]));
    // Grok 有两个窗口
    assert_eq!(rows[3]["windows"].as_array().unwrap().len(), 2);
    // 输入未被修改
    assert_eq!(input, before);
}

// 卡片投影给出总量、无障碍文案与过滤后的额度
#[test]
fn card_projection_provides_total_accessibility_and_filtered_balances() {
    let mut input = snapshot();
    // 只启用 Claude
    input["enabledAgents"] = json!(["claude"]);
    let projection = card_projection(&input);
    // 紧凑与精确总量
    assert_eq!(projection["total"], "18446744073.71B");
    // 无障碍朗读文本
    assert_eq!(projection["exact"], "18,446,744,073,709,551,615");
    assert_eq!(
        projection["amountAccessibility"],
        "Token 18446744073709551615，部分统计，推算"
    );
    // 没有 Claude 额度，所以供应商为空
    assert_eq!(projection["providers"], json!([]));
    // 只有 Claude 一行
    assert_eq!(projection["rows"].as_array().unwrap().len(), 1);
    // 没有快照时的投影
    let absent = card_projection(&Value::Null);
    assert_eq!(absent["hasTokens"], false);
    assert_eq!(absent["total"], "");
    assert_eq!(absent["rows"], json!([]));
    assert_eq!(absent["missingText"], "等待同步");
}

// 空快照（null）投影出没有 Token 的卡片
#[test]
fn null_snapshot_projects_a_card_without_tokens() {
    assert_eq!(card_projection(&serde_json::Value::Null)["hasTokens"], false);
}
