//! 有限的重试决策,由上下文投递与内容寻址的文件投递共用。
use std::time::{Duration, Instant};

/// 每次更新或显式的系统恢复事件最多尝试三次。
// 每次更新或显式系统恢复事件最多尝试三次
const MAX_ATTEMPTS: u8 = 3;

#[derive(Default)]
pub(crate) struct Retry {
    attempts: u8,
    // 最早的下次尝试时间
    after: Option<Instant>,
}

impl Retry {
    // 是否可以尝试：未超次数且已过退避时间
    pub(crate) fn ready(&self, now: Instant) -> bool {
        // 尝试次数未超限且已到重试时刻
        self.attempts < MAX_ATTEMPTS && self.after.is_none_or(|after| now >= after)
    }

    // 开始一次尝试
    pub(crate) fn started(&mut self) {
        self.attempts = self.attempts.saturating_add(1);
        self.after = None;
    }

    // 失败后按 5、10、20、40 秒退避；达到次数上限则不再重试
    pub(crate) fn failed(&mut self, now: Instant) {
        // 退避秒数：5 秒起，最多放大 8 倍
        let seconds = 5_u64 << self.attempts.saturating_sub(1).min(3);
        // 未超限才设定下次重试时刻
        self.after = (self.attempts < MAX_ATTEMPTS).then_some(now + Duration::from_secs(seconds));
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.after
    }
}

/// 较旧的文件可以满足较新清单的相同内容哈希,但绝不满足其状态。
// 旧文件可以满足新清单里相同内容摘要的需求，但绝不能满足它的状态：必须被引用、同一数据流，且修订号不超过当前
pub(crate) fn reusable(
    stream: &str,
    revision: u64,
    current_stream: &str,
    current_revision: u64,
    referenced: bool,
) -> bool {
    // 被引用、同一流且修订号在有效范围内
    referenced && stream == current_stream && revision > 0 && revision <= current_revision
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod tests;
