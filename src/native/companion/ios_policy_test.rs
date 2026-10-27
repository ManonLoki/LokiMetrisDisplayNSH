use super::*;

#[test]
fn retries_back_off_and_stop_without_external_recovery() {
    let mut retry = Retry::default();
    let now = Instant::now();
    for seconds in [5, 10, 0] {
        assert!(retry.ready(now + Duration::from_secs(100)));
        retry.started();
        retry.failed(now);
        assert!(!retry.ready(now));
        if seconds > 0 {
            assert_eq!(retry.deadline(), Some(now + Duration::from_secs(seconds)));
            assert!(retry.ready(now + Duration::from_secs(seconds)));
        }
    }
    assert_eq!(retry.deadline(), None);
    assert!(!retry.ready(now + Duration::from_secs(86400)));
    assert!(Retry::default().ready(now));
}

#[test]
fn files_cannot_cross_streams_or_restore_cleared_data() {
    assert!(reusable("a", 1, "a", 2, true));
    assert!(!reusable("a", 3, "a", 2, true));
    assert!(!reusable("a", 1, "b", 2, true));
    assert!(!reusable("a", 1, "a", 2, false));
    assert!(!reusable("a", 0, "a", 2, true));
}
