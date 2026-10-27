//! Scene 通知可能早于 activationState 更新；显式事件覆盖同次系统快照。
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub(super) enum SceneChange {
    Foreground(usize),
    Background(usize),
    Disconnected(usize),
}

#[derive(Debug, Default)]
pub(super) struct SceneVisibility {
    observed_scene: bool,
}

impl SceneVisibility {
    pub(super) fn sample(
        &mut self,
        application_visible: bool,
        scenes: impl IntoIterator<Item = (usize, bool)>,
        change: Option<SceneChange>,
    ) -> bool {
        let mut scenes: BTreeMap<_, _> = scenes.into_iter().collect();
        self.observed_scene |= !scenes.is_empty() || change.is_some();
        match change {
            Some(SceneChange::Foreground(scene)) => {
                scenes.insert(scene, true);
            }
            Some(SceneChange::Background(scene)) => {
                scenes.insert(scene, false);
            }
            Some(SceneChange::Disconnected(scene)) => {
                scenes.remove(&scene);
            }
            None => {}
        }
        if self.observed_scene {
            scenes.values().any(|visible| *visible)
        } else {
            application_visible
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod ios_state_test;
