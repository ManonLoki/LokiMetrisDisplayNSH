use super::*;
use crate::{
    summary::{
        from_message,
        summary_test::{SOURCE, message},
    },
    view,
};

#[test]
fn no_cache_and_failed_token_data_do_not_claim_zero_or_success() {
    let empty = describe(&json!({"view":null}));
    assert_eq!(empty["widgetToken"]["amount"], "—");
    assert_eq!(empty["tokenMessage"], "点击打开应用并配对");
    assert_eq!(empty["syncDescription"], "尚未同步");
    assert_eq!(empty["watchWaitingTitle"], "等待 iPhone 同步");
    assert_eq!(empty["widgetEmptyInline"], "诡秘神谕 · 尚未同步");
    let failed = view::project(
        &json!({"tokens":null,"tokenError":"unavailable","hooks":[],"hooksAvailable":false}),
    );
    assert_eq!(failed["tokenState"], "unavailable");
    assert_eq!(failed["tokenMessage"], "Token 暂不可用");
    let cached = describe(&json!({"view":failed}));
    assert!(
        cached["tokenMessage"].is_null(),
        "an existing cache must not ask the user to pair again"
    );
    assert_eq!(cached["widgetToken"]["amount"], "不可用");
    assert_eq!(cached["widgetToken"]["quality"], "Token 暂不可用");
    assert_eq!(cached["hooksWidgetFallback"], "Hooks 暂不可用");
}

#[test]
fn descriptions_preserve_exact_amount_and_use_host_formatted_dates() {
    let record = from_message(&message(), SOURCE, 1).unwrap();
    let view = view::project(&record["snapshot"]);
    let labels = describe(
        &json!({"view":view,"appName":"测试", "syncTime":"27 Oct, 11:30", "compactSyncTime":"11:30", "quotaTime":"27 Oct, 10:00"}),
    );
    assert_eq!(labels["syncText"], "上次同步 27 Oct, 11:30");
    assert_eq!(labels["compactSyncText"], "上次 11:30");
    assert_eq!(labels["widgetToken"]["precise"], "18446744073709551615");
    let root = labels["rootDescription"].as_str().unwrap();
    assert!(root.starts_with("测试，Token 用量：18446744073709551615，部分·估算。"));
    assert!(root.contains("Codex，套餐 Plus"));
    assert!(root.contains("运行 1"));
    assert!(root.contains("最近同步 27 Oct, 11:30。"));
    assert!(
        labels["providerDescriptions"]["codex"]
            .as_str()
            .unwrap()
            .ends_with("额度更新于 27 Oct, 10:00")
    );
}

#[test]
fn exact_quality_and_hook_accessibility_are_projected_once() {
    let mut message = message();
    message["snapshot"]["tokens"]["all"] = json!({"total":"5","availability":"available","completeness":"complete","confidence":"exact"});
    let view = view::project(&from_message(&message, SOURCE, 1).unwrap()["snapshot"]);
    assert_eq!(view["widgetToken"]["quality"], "完整 · 精确");
    assert_eq!(view["tokens"]["total"]["accessibilityLabel"], "总 Token，5");
    assert_eq!(view["hooks"][0]["accessibilityLabel"], "Codex，运行");
    assert_eq!(view["hookLead"], "· Codex 运行");
}

#[test]
fn widget_compact_quota_and_hidden_agents_use_host_layout_facts() {
    let view = json!({"agents":[{},{},{}]});
    let labels = describe(&json!({"view":view,"quotaCompactTime":"10/27 12:20","visibleAgents":2}));
    assert_eq!(labels["quotaCompactUpdate"], "更新 10/27 12:20");
    assert_eq!(labels["remainingAgents"], "另 1 个 Agent");
    let labels = describe(&json!({"view":view,"visibleAgents":4}));
    assert_eq!(labels["remainingAgents"], "");
    assert_eq!(labels["quotaCompactUpdate"], "");
}
