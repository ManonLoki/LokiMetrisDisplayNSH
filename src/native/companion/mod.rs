//! 镜像到已配对 Apple Watch 的公开、不含凭据的快照。
// 手表发件箱：写入公开快照
mod watch_outbox;

// 非 iOS 的测试也编译 iOS 文件与策略模块
#[cfg(all(test, not(target_os = "ios")))]
#[path = "ios/files.rs"]
mod ios_files;
#[cfg(all(test, not(target_os = "ios")))]
#[path = "ios/policy.rs"]
mod ios_policy;

#[cfg(target_os = "ios")]
// iOS 的手表会话
mod ios;

/// 启动进程范围的手表投递;其他目标没有配对的手表。
// 启动进程级的手表投递；其他平台没有配对手表
pub(crate) fn install() {
    #[cfg(target_os = "ios")]
    ios::install();
}
#[cfg(target_os = "ios")]
// iOS 上导出清单类型
pub(super) use watch_outbox::{Manifest, read_manifest};

// 引入 JSON 值
use serde_json::Value;
// 引入路径
use std::path::Path;

/// 由已认证的连接所有者调用,绝不由 UI 轮询定时器调用。
// 由经认证的连接 owner 调用（不由界面轮询调用）：更新手表的公开快照
pub(super) fn publish(
    // 根目录
    root: &Path,
    // 来源
    source: Option<&str>,
    // 快照
    snapshot: Option<&Value>,
    // 接收时间
    received: Option<u64>,
    // 无返回值
) {
    // 发件箱目录
    let directory = root.join("companion-v1");
    // 更新发件箱
    match watch_outbox::update(&directory, root, source, snapshot, received) {
        // 有变化
        Ok(changed) => {
            // iOS：有变化则通知手表会话
            #[cfg(target_os = "ios")]
            if changed {
                // 通知 iOS 配套
                ios::changed(&directory);
            }
            #[cfg(not(target_os = "ios"))]
            let _ = changed;
        }
        // 公开缓存写入失败不应打断桌面会话；下次认证更新会重试，不会发送写了一半的文件
        Err(_) => {
            // 公开缓存写入失败时,绝不能打断桌面会话。
            // 下一次已认证的更新会重试;不会发送写了一半的文件。
        }
    }
}
