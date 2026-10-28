//! 展示模型中的描述模板；日期只接受宿主已按本机区域格式化的公开文字。
// 引入 JSON
use serde_json::{Value, json};

// 取字符串，非字符串视为空
pub(super) fn string(value: &Value) -> &str {
    // 字符串或空串
    value.as_str().unwrap_or_default()
}

// 文本加质量说明；无说明则原样返回
pub(super) fn qualified(text: &str, quality: &str) -> String {
    // 没有质量说明
    if quality.is_empty() {
        // 原文本
        text.to_owned()
    // 否则分支
    } else {
        // 文本加质量说明
        format!("{text}，{quality}")
    }
}

// 生成所有描述文案；日期只接受宿主已按本机区域格式化的公开文字
pub(super) fn describe(request: &Value) -> Value {
    // 视图
    let view = &request["view"];
    // 用量
    let tokens = &view["tokens"];
    // 读取非空的格式化时间文本
    let stamp = |key: &str| request[key].as_str().filter(|text| !text.is_empty());
    // 同步时间
    let sync = stamp("syncTime");
    // 额度更新时间
    let quota_time = stamp("quotaTime");
    // 因空间不足被隐藏的代理数
    let hidden_agents = request["visibleAgents"].as_u64().and_then(|visible| {
        // 代理总数
        let count = view["agents"].as_array()?.len() as u64;
        // 减去已显示数，大于零才返回
        count.checked_sub(visible).filter(|count| *count > 0)
    });
    // 各供应商的描述，附上额度更新时间
    let provider_descriptions: serde_json::Map<String, Value> = view["quota"]["providers"]
        // 转为数组
        .as_array()
        // 转为迭代器
        .into_iter()
        // 展平
        .flatten()
        // 逐个供应商
        .map(|provider| {
            // 供应商描述
            let description = string(&provider["description"]);
            // 有更新时间则附上
            let description = quota_time.map_or_else(
                // 没有更新时间则用原描述
                || description.to_owned(),
                // 附上额度更新时间
                |stamp| format!("{description}，额度更新于 {stamp}"),
            );
            // 以供应商标识为键
            (string(&provider["provider"]).to_owned(), json!(description))
        })
        // 收集为映射
        .collect();
    // “上次同步于 …”或“尚未同步”
    let apple_sync = sync.map_or_else(
        // 没有同步时间则显示“尚未同步”
        || "尚未同步".to_owned(),
        // 否则显示“上次同步于 …”
        |stamp| format!("上次同步于 {stamp}"),
    );
    // 紧凑形式使用的时间
    let compact_stamp = stamp("compactSyncTime").or(sync);
    // 紧凑形式的无障碍描述
    let compact_description = if tokens.is_object() {
        // 日期的读屏写法加同步描述
        format!("{}，{apple_sync}", string(&tokens["dateAccessibility"]))
    // 否则分支
    } else {
        // 否则只有同步描述
        apple_sync.clone()
    };
    // 紧凑形式的文字：日期加同步时间
    let compact_text = compact_stamp.map_or_else(
        // 没有同步时间则显示“尚未同步”
        || "尚未同步".to_owned(),
        // 有则格式化
        |stamp| {
            // 有用量
            if tokens.is_object() {
                // 日期加同步时间
                format!("{} · 同步 {stamp}", string(&tokens["date"]))
            // 否则分支
            } else {
                // 否则只有同步时间
                format!("同步 {stamp}")
            }
        },
    );
    // 应用名称，默认“诡秘神谕”
    let app_name = request["appName"].as_str().unwrap_or("诡秘神谕");
    // 组装文案结构
    json!({
        // 空状态文案
        "widgetEmptyInline": format!("{app_name} · 尚未同步"),
        // 圆形空态与标题
        "widgetEmptyCircular": "未同步", "widgetEmptyTitle": "尚未同步",
        // 空态帮助
        "widgetEmptyHelp": "打开 App 连接桌面",
        // 额度更新说明
        "quotaUpdate": quota_time.map_or_else(String::new, |stamp| format!("额度更新于 {stamp}")),
        // 紧凑的额度更新说明
        "quotaCompactUpdate": stamp("quotaCompactTime").map_or_else(String::new, |stamp| format!("更新 {stamp}")),
        // 被隐藏代理的提示
        "remainingAgents": hidden_agents.map_or_else(String::new, |count| format!("另 {count} 个 Agent")),
        // 各供应商的描述
        "providerDescriptions": provider_descriptions,
        // 各表面使用的同步文案
        "appleSyncText": compact_stamp.map_or_else(|| "尚未同步".to_owned(), |stamp| format!("上次同步 {stamp}")),
        // 同步描述
        "appleSyncDescription": apple_sync,
        // 紧凑同步文案与描述
        "appleCompactSyncText": compact_text, "appleCompactSyncDescription": compact_description,
        // 锁屏小组件的内联与圆形文案
        "appleAccessoryInline": compact_stamp.map_or_else(|| string(&view["hooksText"]).to_owned(), |stamp| format!("上次 {stamp} · {}", string(&view["hooksText"]))),
        // 锁屏圆形文案
        "appleAccessoryCircular": format!("{apple_sync}，{}", string(&view["hooksText"])),
    })
}

// 测试模块
#[cfg(test)]
#[path = "view_text_test.rs"]
mod view_text_test;
