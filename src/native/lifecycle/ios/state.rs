//! Scene 通知可能早于 activationState 更新；显式事件覆盖同次系统快照。
// 引入有序映射
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
// 场景的显式变化
pub(super) enum SceneChange {
    // 前台场景
    Foreground(usize),
    // 后台场景
    Background(usize),
    // 断开场景
    Disconnected(usize),
}

#[derive(Debug, Default)]
// 场景可见性状态
pub(super) struct SceneVisibility {
    // 是否曾观察到场景；之后应用级状态不再作为依据
    observed_scene: bool,
}

// 场景可见性的方法
impl SceneVisibility {
    // 采样：场景通知可能早于 activationState 更新，因此显式事件覆盖同一次系统快照
    pub(super) fn sample(
        // 自身
        &mut self,
        // 应用是否可见
        application_visible: bool,
        // 场景集合
        scenes: impl IntoIterator<Item = (usize, bool)>,
        // 变更
        change: Option<SceneChange>,
        // 返回是否可见
    ) -> bool {
        // 场景可见性映射
        let mut scenes: BTreeMap<_, _> = scenes.into_iter().collect();
        // 已观察过场景
        self.observed_scene |= !scenes.is_empty() || change.is_some();
        // 按变更类型处理
        match change {
            // 强制记为可见
            Some(SceneChange::Foreground(scene)) => {
                // 标记场景可见
                scenes.insert(scene, true);
            }
            // 强制记为不可见
            Some(SceneChange::Background(scene)) => {
                // 标记场景不可见
                scenes.insert(scene, false);
            }
            // 移除该场景
            Some(SceneChange::Disconnected(scene)) => {
                // 移除断开的场景
                scenes.remove(&scene);
            }
            // 无变更
            None => {}
        }
        // 有场景时：任一场景可见即可见
        if self.observed_scene {
            // 任一场景可见即可见
            scenes.values().any(|visible| *visible)
        // 还没有场景：退回应用级状态
        } else {
            // 没有场景时取应用自身的可见性
            application_visible
        }
    }
}

// 测试模块
#[cfg(test)]
#[path = "state_test.rs"]
mod ios_state_test;
