// 引入被测文案模块
use super::*;
use crate::{
    summary::{
        from_message,
        summary_test::{SOURCE, message},
    },
    view,
};

#[test]
// 没有缓存或 Token 失败时，不宣称为 0 或成功
fn no_cache_and_failed_token_data_do_not_claim_zero_or_success() {
    // 没有缓存
    let empty = describe(&json!({"view":null}));
    // 空态文案使用默认应用名
    assert_eq!(empty["widgetEmptyInline"], "诡秘神谕 · 尚未同步");
    assert_eq!(empty["appleSyncDescription"], "尚未同步");
    assert_eq!(empty["appleCompactSyncText"], "尚未同步");
    // Token 失败的视图
    let failed = view::project(
        &json!({"tokens":null,"tokenError":"unavailable","hooks":[],"hooksAvailable":false}),
    );
    // 不宣称为 0 或成功
    assert_eq!(failed["tokenFallback"], "不可用");
    assert_eq!(failed["tokenAccessibilityLabel"], "总 Token，不可用");
    let cached = describe(&json!({"view":failed}));
    assert_eq!(cached["appleAccessoryCircular"], "尚未同步，Hooks 未同步");
}

#[test]
// 描述文案使用宿主格式化好的日期
fn descriptions_use_host_formatted_dates() {
    let record = from_message(&message(), SOURCE, 1).unwrap();
    let view = view::project(&record["snapshot"]);
    // 宿主提供已按本机区域格式化的日期文本
    let labels = describe(
        &json!({"view":view,"appName":"测试", "syncTime":"27 Oct, 11:30", "compactSyncTime":"11:30", "quotaTime":"27 Oct, 10:00"}),
    );
    // 各表面使用的同步文案
    assert_eq!(labels["appleSyncText"], "上次同步 11:30");
    assert_eq!(labels["appleSyncDescription"], "上次同步于 27 Oct, 11:30");
    assert_eq!(labels["appleAccessoryInline"], "上次 11:30 · Hooks 1");
    // 应用名取自参数
    assert_eq!(labels["widgetEmptyInline"], "测试 · 尚未同步");
    // 紧凑描述以“统计日期”开头
    assert!(
        labels["appleCompactSyncDescription"]
            .as_str()
            .unwrap()
            .starts_with("统计日期 ")
    );
    // 供应商描述附上额度更新时间
    assert!(
        labels["providerDescriptions"]["codex"]
            .as_str()
            .unwrap()
            .ends_with("额度更新于 27 Oct, 10:00")
    );
}

#[test]
// 精确值与首条 Hook 只投影一次
fn exact_amount_and_hook_lead_are_projected_once() {
    // 总量精确值只有 5
    let mut message = message();
    message["snapshot"]["tokens"]["all"] = json!({"total":"5","availability":"available","completeness":"complete","confidence":"exact"});
    let view = view::project(&from_message(&message, SOURCE, 1).unwrap()["snapshot"]);
    // 精确值与无障碍文案只投影一次
    assert_eq!(view["tokens"]["total"]["exact"], "5");
    assert_eq!(view["tokens"]["total"]["accessibilityLabel"], "总 Token，5");
    // 首条 Hook 摘要
    assert_eq!(view["hookLead"], "· Codex 运行");
}

#[test]
// 紧凑额度与被隐藏的代理依据宿主的布局事实
fn widget_compact_quota_and_hidden_agents_use_host_layout_facts() {
    // 三个代理
    let view = json!({"agents":[{},{},{}]});
    // 只显示两个时，提示另有 1 个，并给出紧凑的额度更新时间
    let labels = describe(&json!({"view":view,"quotaCompactTime":"10/27 12:20","visibleAgents":2}));
    assert_eq!(labels["quotaCompactUpdate"], "更新 10/27 12:20");
    assert_eq!(labels["remainingAgents"], "另 1 个 Agent");
    // 全部可见时没有提示，也没有额度更新
    let labels = describe(&json!({"view":view,"visibleAgents":4}));
    assert_eq!(labels["remainingAgents"], "");
    assert_eq!(labels["quotaCompactUpdate"], "");
}
