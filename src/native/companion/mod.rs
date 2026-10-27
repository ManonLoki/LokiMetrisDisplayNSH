//! Public, credential-free snapshots mirrored to the paired Apple Watch.
mod store;

#[cfg(all(test, not(target_os = "ios")))]
#[path = "ios_files.rs"]
mod ios_files;
#[cfg(all(test, not(target_os = "ios")))]
#[path = "ios_policy.rs"]
mod ios_policy;

#[cfg(target_os = "ios")]
mod ios;

#[cfg(target_os = "ios")]
pub(crate) use ios::install;
#[cfg(target_os = "ios")]
pub(super) use store::{Manifest, read_manifest};

use serde_json::Value;
use std::path::Path;

/// Called by the authenticated connection owner, never by a UI polling timer.
pub(super) fn publish(
    root: &Path,
    source: Option<&str>,
    snapshot: Option<&Value>,
    received: Option<u64>,
) {
    let directory = root.join("companion-v1");
    match store::update(&directory, root, source, snapshot, received) {
        Ok(changed) => {
            #[cfg(target_os = "ios")]
            if changed {
                ios::changed(&directory);
            }
            #[cfg(not(target_os = "ios"))]
            let _ = changed;
        }
        Err(_) => {
            // A failed public-cache write must never interrupt the desktop session.
            // The next authenticated update retries; no partially written file is sent.
        }
    }
}
