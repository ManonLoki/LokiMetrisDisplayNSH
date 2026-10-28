// 引入被测类型
use super::{SceneChange, SceneVisibility};

#[test]
// 后台启动且没有场景时，以应用级状态为准
fn background_launch_without_scenes_uses_the_application_fact() {
    let mut visibility = SceneVisibility::default();
    // 没有场景时，以应用级状态为准：后台为不可见
    assert!(!visibility.sample(false, [], None));
    // 前台为可见
    assert!(visibility.sample(true, [], None));
}

#[test]
// 只要有一个前台场景，进程就是可见的
fn one_foreground_scene_keeps_the_process_visible() {
    let mut visibility = SceneVisibility::default();
    // 有一个场景在前台，即使应用级状态为后台，也算可见
    assert!(visibility.sample(false, [(1, true), (2, false)], None));
    // 场景 2 收到后台通知，但场景 1 仍在前台
    assert!(visibility.sample(
        true,
        [(1, true), (2, true)],
        Some(SceneChange::Background(2))
    ));
    // 场景 1 也已不可见，场景 2 的后台通知覆盖了系统快照里的前台：整体不可见
    assert!(!visibility.sample(
        true,
        [(1, false), (2, true)],
        Some(SceneChange::Background(2))
    ));
}

#[test]
// 前台通知会覆盖 UIKit 尚未更新的快照
fn foreground_notice_overrides_a_snapshot_not_yet_updated_by_uikit() {
    let mut visibility = SceneVisibility::default();
    // 前台通知覆盖 UIKit 尚未更新的快照
    assert!(visibility.sample(false, [(7, false)], Some(SceneChange::Foreground(7))));
    // 快照里甚至还没有该场景
    assert!(visibility.sample(false, [], Some(SceneChange::Foreground(8))));
}

#[test]
// 断开最后一个场景后，不会沿用过期的应用级状态
fn disconnecting_the_last_scene_does_not_reuse_a_stale_application_state() {
    let mut visibility = SceneVisibility::default();
    // 先有一个前台场景
    assert!(visibility.sample(true, [(1, true)], None));
    // 它断开后，即使应用级状态是前台也不可见
    assert!(!visibility.sample(true, [(1, true)], Some(SceneChange::Disconnected(1))));
    // 没有场景了，仍不沿用过期的应用级状态
    assert!(!visibility.sample(true, [], None));
    // 新场景出现并在前台，则可见
    assert!(visibility.sample(false, [(2, true)], None));
}

#[test]
// 断开一个窗口时，其他前台窗口仍保持可见
fn disconnecting_one_window_keeps_other_foreground_windows_visible() {
    let mut visibility = SceneVisibility::default();
    // 两个前台窗口断开其中一个，另一个仍可见
    assert!(visibility.sample(
        false,
        [(1, true), (2, true)],
        Some(SceneChange::Disconnected(1))
    ));
}
