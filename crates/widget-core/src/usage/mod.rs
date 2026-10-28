//! 共享的应用格式化逻辑,以及小组件受限的文本呈现。
// 复用主应用的代理匹配与排序
#[path = "../../../../src/features/usage/agents.rs"]
pub mod agents;
// 复用主应用的额度投影
#[path = "../../../../src/features/usage/quota.rs"]
pub mod quota;
// 复用主应用的格式化，并追加小组件专用的简短说明
pub mod formatting {
    // 引入生成的共享源码
    include!(concat!(
        // 工程目录
        env!("CARGO_MANIFEST_DIR"),
        // 相对路径指向共享的格式化源码
        "/../../src/features/usage/formatting.rs"
    ));
    /// 完整无障碍标签放不下时使用的简短说明。
    // 简短的质量说明，用于放不下完整无障碍文案的地方
    pub fn amount_quality(amount: &Value) -> String {
        // 完整度：部分或范围未知
        let completeness = match amount["completeness"].as_str() {
            // 部分统计
            Some("partial") => Some("部分"),
            // 范围未知
            Some("unknown") => Some("范围未知"),
            // 其余无说明
            _ => None,
        };
        // 置信度：估算或待核对
        let confidence = match amount["confidence"].as_str() {
            // 估算值
            Some("derived") => Some("估算"),
            // 待核对
            Some("suspected") => Some("待核对"),
            // 其余无说明
            _ => None,
        };
        // 完整性与置信度
        [completeness, confidence]
            // 转为迭代器
            .into_iter()
            // 展平
            .flatten()
            // 收集为列表
            .collect::<Vec<_>>()
            // 用“·”连接
            .join("·")
    }
}
