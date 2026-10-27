use super::*;

fn snapshot() -> Value {
    let mut agents = Vec::new();
    for name in [
        "Other B",
        "WorkBuddy",
        "Grok Build",
        "Claude Code",
        "codex",
        "Other A",
        "SecretBot",
    ] {
        agents.push(
            json!({"agent":name,"amount":{"total":"1500","availability":"available",
            "completeness":"complete","confidence":"exact"}}),
        );
    }
    json!({"tokens":{"day":{"date":"2026-10-26","zone":"Asia/Shanghai"},
        "all":{"total":"18446744073709551615","availability":"available",
            "completeness":"partial","confidence":"derived"},"agents":agents},
        "enabledAgents":["codex","claude","cursor","grok","workbuddy","Other B","Other A"],
        "tokenError":null,"providerQuota":{"observedAtUnix":1793019996,"providers":[
            {"provider":"codex","plan":"plus","weekly":{"kind":"weekly","usedTenths":500}},
            {"provider":"grok","plan":null,"monthly":{"kind":"monthly","usedTenths":800},
                "fiveHour":{"kind":"fiveHour","usedTenths":300}}]}})
}

#[test]
fn display_projection_merges_aliases_and_filters_disabled_agents() {
    let input = snapshot();
    let before = input.clone();
    let rows = display_rows(&input);
    assert_eq!(
        rows.iter()
            .map(|row| row["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "Codex",
            "Claude Code",
            "Cursor",
            "Grok",
            "Other B",
            "Other A",
            "WorkBuddy"
        ]
    );
    assert_eq!(rows[0]["windows"][0]["kindLabel"], "每周");
    assert_eq!(rows[2]["tokenText"], "0");
    assert_eq!(rows[2]["windows"], json!([]));
    assert_eq!(rows[3]["windows"].as_array().unwrap().len(), 2);
    assert_eq!(input, before);
}

#[test]
fn card_projection_provides_total_accessibility_and_filtered_balances() {
    let mut input = snapshot();
    input["enabledAgents"] = json!(["claude"]);
    let projection = card_projection(&input);
    assert_eq!(projection["total"], "18446744073.71B");
    assert_eq!(projection["exact"], "18,446,744,073,709,551,615");
    assert_eq!(
        projection["amountAccessibility"],
        "Token 18446744073709551615，部分统计，推算"
    );
    assert_eq!(projection["providers"], json!([]));
    assert_eq!(projection["rows"].as_array().unwrap().len(), 1);
    let absent = card_projection(&Value::Null);
    assert_eq!(absent["hasTokens"], false);
    assert_eq!(absent["total"], "");
    assert_eq!(absent["rows"], json!([]));
    assert_eq!(absent["missingText"], "等待同步");
}
