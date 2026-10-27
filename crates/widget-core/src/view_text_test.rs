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
    assert_eq!(empty["widgetEmptyInline"], "诡秘神谕 · 尚未同步");
    assert_eq!(empty["appleSyncDescription"], "尚未同步");
    assert_eq!(empty["appleCompactSyncText"], "尚未同步");
    let failed = view::project(
        &json!({"tokens":null,"tokenError":"unavailable","hooks":[],"hooksAvailable":false}),
    );
    assert_eq!(failed["tokenFallback"], "不可用");
    assert_eq!(failed["tokenAccessibilityLabel"], "总 Token，不可用");
    let cached = describe(&json!({"view":failed}));
    assert_eq!(cached["appleAccessoryCircular"], "尚未同步，Hooks 未同步");
}

#[test]
fn descriptions_use_host_formatted_dates() {
    let record = from_message(&message(), SOURCE, 1).unwrap();
    let view = view::project(&record["snapshot"]);
    let labels = describe(
        &json!({"view":view,"appName":"测试", "syncTime":"27 Oct, 11:30", "compactSyncTime":"11:30", "quotaTime":"27 Oct, 10:00"}),
    );
    assert_eq!(labels["appleSyncText"], "上次同步 11:30");
    assert_eq!(labels["appleSyncDescription"], "上次同步于 27 Oct, 11:30");
    assert_eq!(labels["appleAccessoryInline"], "上次 11:30 · Hooks 1");
    assert_eq!(labels["widgetEmptyInline"], "测试 · 尚未同步");
    assert!(
        labels["appleCompactSyncDescription"]
            .as_str()
            .unwrap()
            .starts_with("统计日期 ")
    );
    assert!(
        labels["providerDescriptions"]["codex"]
            .as_str()
            .unwrap()
            .ends_with("额度更新于 27 Oct, 10:00")
    );
}

#[test]
fn exact_amount_and_hook_lead_are_projected_once() {
    let mut message = message();
    message["snapshot"]["tokens"]["all"] = json!({"total":"5","availability":"available","completeness":"complete","confidence":"exact"});
    let view = view::project(&from_message(&message, SOURCE, 1).unwrap()["snapshot"]);
    assert_eq!(view["tokens"]["total"]["exact"], "5");
    assert_eq!(view["tokens"]["total"]["accessibilityLabel"], "总 Token，5");
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
