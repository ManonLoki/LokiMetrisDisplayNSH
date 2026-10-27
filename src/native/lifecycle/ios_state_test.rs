use super::{SceneChange, SceneVisibility};

#[test]
fn background_launch_without_scenes_uses_the_application_fact() {
    let mut visibility = SceneVisibility::default();
    assert!(!visibility.sample(false, [], None));
    assert!(visibility.sample(true, [], None));
}

#[test]
fn one_foreground_scene_keeps_the_process_visible() {
    let mut visibility = SceneVisibility::default();
    assert!(visibility.sample(false, [(1, true), (2, false)], None));
    assert!(visibility.sample(
        true,
        [(1, true), (2, true)],
        Some(SceneChange::Background(2))
    ));
    assert!(!visibility.sample(
        true,
        [(1, false), (2, true)],
        Some(SceneChange::Background(2))
    ));
}

#[test]
fn foreground_notice_overrides_a_snapshot_not_yet_updated_by_uikit() {
    let mut visibility = SceneVisibility::default();
    assert!(visibility.sample(false, [(7, false)], Some(SceneChange::Foreground(7))));
    assert!(visibility.sample(false, [], Some(SceneChange::Foreground(8))));
}

#[test]
fn disconnecting_the_last_scene_does_not_reuse_a_stale_application_state() {
    let mut visibility = SceneVisibility::default();
    assert!(visibility.sample(true, [(1, true)], None));
    assert!(!visibility.sample(true, [(1, true)], Some(SceneChange::Disconnected(1))));
    assert!(!visibility.sample(true, [], None));
    assert!(visibility.sample(false, [(2, true)], None));
}

#[test]
fn disconnecting_one_window_keeps_other_foreground_windows_visible() {
    let mut visibility = SceneVisibility::default();
    assert!(visibility.sample(
        false,
        [(1, true), (2, true)],
        Some(SceneChange::Disconnected(1))
    ));
}
