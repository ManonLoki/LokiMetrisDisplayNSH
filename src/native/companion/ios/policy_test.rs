// 引入被测模块
use super::*;

#[test]
// 重试会退避，并在没有外部恢复事件时停止
fn retries_back_off_and_stop_without_external_recovery() {
    let mut retry = Retry::default();
    let now = Instant::now();
    // 三次尝试：失败后依次退避 5 秒、10 秒，第三次后不再重试
    for seconds in [5, 10, 0] {
        assert!(retry.ready(now + Duration::from_secs(100)));
        // 开始一次尝试并失败
        retry.started();
        retry.failed(now);
        // 失败后立即不可重试
        assert!(!retry.ready(now));
        if seconds > 0 {
            // 退避期限正确，到期后可再次尝试
            assert_eq!(retry.deadline(), Some(now + Duration::from_secs(seconds)));
            assert!(retry.ready(now + Duration::from_secs(seconds)));
        }
    }
    // 三次用完：没有期限
    assert_eq!(retry.deadline(), None);
    // 即使过了很久也不再重试，必须等外部恢复事件
    assert!(!retry.ready(now + Duration::from_secs(86400)));
    // 新的重试状态则可以立即尝试
    assert!(Retry::default().ready(now));
}

#[test]
// 文件不能跨数据流，也不能让已清除的数据复活
fn files_cannot_cross_streams_or_restore_cleared_data() {
    // 同一流、修订号不超过当前、被引用：可复用
    assert!(reusable("a", 1, "a", 2, true));
    // 修订号比当前更新：不可复用
    assert!(!reusable("a", 3, "a", 2, true));
    // 不同的流：不可复用
    assert!(!reusable("a", 1, "b", 2, true));
    // 未被引用（如已清除）：不可复用
    assert!(!reusable("a", 1, "a", 2, false));
    // 修订号为 0：不可复用
    assert!(!reusable("a", 0, "a", 2, true));
}
