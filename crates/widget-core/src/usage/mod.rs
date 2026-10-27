//! Shared application formatting plus the Widget's constrained text presentations.
#[path = "../../../../src/features/usage/agents.rs"]
pub mod agents;
#[path = "../../../../src/features/usage/quota.rs"]
pub mod quota;
pub mod formatting {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../src/features/usage/formatting.rs"
    ));
    /// Short disclosure used where the full accessibility label does not fit.
    pub fn amount_quality(amount: &Value) -> String {
        let completeness = match amount["completeness"].as_str() {
            Some("partial") => Some("部分"),
            Some("unknown") => Some("范围未知"),
            _ => None,
        };
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
