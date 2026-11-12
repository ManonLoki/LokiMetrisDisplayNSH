//! 共享的应用格式化逻辑,以及小组件受限的文本呈现。
// 复用主应用的代理匹配与排序
#[path = "../../../../src/features/usage/agents.rs"]
pub mod agents;
// 复用主应用的额度投影
#[path = "../../../../src/features/usage/quota.rs"]
pub mod quota;
// 复用主应用的格式化，并追加小组件专用的简短说明
pub mod formatting {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        // 相对路径指向共享的格式化源码
        "/../../src/features/usage/formatting.rs"
    ));
    /// 完整无障碍标签放不下时使用的简短说明。
    // 简短的质量说明，用于放不下完整无障碍文案的地方
    pub fn amount_quality(amount: &Value) -> String {
        // 完整度：部分或范围未知
        let completeness = match amount["completeness"].as_str() {
            Some("partial") => Some("部分"),
            Some("unknown") => Some("范围未知"),
            _ => None,
        };
        // 置信度：估算或待核对
        let confidence = match amount["confidence"].as_str() {
            Some("derived") => Some("估算"),
            Some("suspected") => Some("待核对"),
            _ => None,
        };
        [completeness, confidence]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("·")
    }
}
