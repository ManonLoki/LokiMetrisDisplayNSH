//! 所有 UI 共用的 Token 总数、提供方匹配与余额投影。
// 代理匹配与排序
mod agents;
// 数字格式化
mod formatting;
// 额度投影
mod quota;
// 卡片行投影
mod rows;

// 引入 JSON 值
use serde_json::Value;

/// 未知操作属于其他功能;共享桥接层只有在每个功能都有机会
/// 处理该请求之后,才会报告错误。
// 分发用量相关的查询，未知操作返回 None
pub(crate) fn dispatch(op: &str, args: &[Value]) -> Option<Result<Value, String>> {
    // 按操作名分发
    match op {
        // 用量卡片投影，缺少参数时报错
        "usage.cardProjection" => Some(
            // 取第一个参数
            args.first()
                // 缺少则报错
                .ok_or_else(|| "Missing usage argument 0".to_owned())
                // 生成卡片投影
                .map(rows::card_projection),
        ),
        // 其余操作不支持
        _ => None,
    }
}

// 测试模块
#[cfg(test)]
#[path = "dispatch_test.rs"]
mod dispatch_test;
