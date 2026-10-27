//! Versioned public snapshots and immutable, checked asset copies for WatchConnectivity.
use loki_metris_pairing::assets::AssetDescriptor;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::Path,
};

const MAX_MANIFEST: u64 = 48 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Manifest {
    pub version: u32,
    pub stream_id: String,
    pub revision: String,
    pub source_id: Option<String>,
    pub received_at_ms: Option<u64>,
    pub snapshot: Option<Value>,
    pub assets: Vec<AssetDescriptor>,
}

fn failure(_: impl std::fmt::Display) -> String {
    "手表展示缓存不可用".into()
}

/// Stream ids are generated locally by `Uuid::new_v4`; source ids are paired desktop ids.
fn valid_stream_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}

fn project(value: &Value) -> Result<Value, String> {
    let mut snapshot =
        crate::features::connection::snapshot::project_snapshot(value, &serde_json::json!({}))
            .ok_or_else(|| failure("snapshot"))?;
    if let Some(hooks) = snapshot["hooks"].as_array_mut() {
        for hook in hooks {
            if let Some(object) = hook.as_object_mut() {
                object.remove("imageSource");
            }
        }
    }
    Ok(snapshot)
}

impl Manifest {
    pub(super) fn validate(&self) -> Result<(), String> {
        let revision = self.revision.parse::<u64>().map_err(failure)?;
        if self.version != 1
            || !valid_stream_id(&self.stream_id)
            || revision == 0
            || revision.to_string() != self.revision
            || self.assets.len() > 12
        {
            return Err(failure("manifest"));
        }
        match (&self.source_id, &self.snapshot, self.received_at_ms) {
            (None, None, None) if self.assets.is_empty() => return Ok(()),
            (Some(source), Some(snapshot), Some(received))
                if crate::features::connection::identity::valid_device_id(source)
                    && received > 0
                    && received <= 9_007_199_254_740_991
                    && project(snapshot).as_ref() == Ok(snapshot) => {}
            _ => return Err(failure("snapshot")),
        }
        let referenced: BTreeSet<_> = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot["hooks"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|hook| hook["image"].as_str())
            .collect();
        let mut hashes = BTreeSet::new();
        for asset in &self.assets {
            asset.validate().map_err(failure)?;
            if !referenced.contains(asset.hash.as_str()) || !hashes.insert(asset.hash.as_str()) {
                return Err(failure("assets"));
            }
        }
        if hashes != referenced {
            return Err(failure("missing asset"));
        }
        Ok(())
    }
}

fn read_file(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    let meta = fs::symlink_metadata(path).map_err(failure)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > maximum {
        return Err(failure("file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(failure)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(failure)?;
    if bytes.len() as u64 > maximum {
        return Err(failure("length"));
    }
    Ok(bytes)
}

pub(crate) fn read_manifest(root: &Path) -> Result<Manifest, String> {
    let manifest: Manifest =
        serde_json::from_slice(&read_file(&root.join("manifest.json"), MAX_MANIFEST)?)
            .map_err(failure)?;
    manifest.validate()?;
    Ok(manifest)
}

fn directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(failure)?;
    let meta = fs::symlink_metadata(path).map_err(failure)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(failure("directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(failure)?;
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension(format!("{}.pending", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(failure)?;
        file.write_all(bytes).map_err(failure)?;
        file.sync_all().map_err(failure)?;
        drop(file);
        fs::rename(&temp, path).map_err(failure)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

/// Returns whether either the manifest or an available, referenced asset changed.
pub(super) fn update(
    root: &Path,
    link_root: &Path,
    source: Option<&str>,
    raw: Option<&Value>,
    received: Option<u64>,
) -> Result<bool, String> {
    directory(root)?;
    let previous = if root.join("manifest.json").exists() {
        Some(read_manifest(root)?)
    } else {
        None
    };
    let mut manifest = Manifest {
        version: 1,
        stream_id: previous.as_ref().map_or_else(
            || uuid::Uuid::new_v4().to_string(),
            |old| old.stream_id.clone(),
        ),
        revision: previous
            .as_ref()
            .map_or_else(|| "0".into(), |old| old.revision.clone()),
        source_id: None,
        received_at_ms: None,
        snapshot: None,
        assets: Vec::new(),
    };
    if let (Some(source), Some(raw), Some(received)) = (source, raw, received) {
        let snapshot = project(raw)?;
        let referenced: BTreeSet<_> = snapshot["hooks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|hook| hook["image"].as_str())
            .collect();
        let descriptors: Vec<AssetDescriptor> =
            serde_json::from_value(raw["assets"].clone()).map_err(failure)?;
        manifest.assets = descriptors
            .into_iter()
            .filter(|asset| referenced.contains(asset.hash.as_str()))
            .collect();
        manifest.assets.sort_by(|a, b| a.hash.cmp(&b.hash));
        manifest.source_id = Some(source.into());
        manifest.received_at_ms = Some(received);
        manifest.snapshot = Some(snapshot);
    }
    let content_changed = previous.as_ref() != Some(&manifest);
    if content_changed {
        let next = manifest
            .revision
            .parse::<u64>()
            .map_err(failure)?
            .checked_add(1)
            .ok_or_else(|| failure("revision exhausted"))?;
        manifest.revision = next.to_string();
    }
    manifest.validate()?;
    let bytes = serde_json::to_vec(&manifest).map_err(failure)?;
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err(failure("context too large"));
    }
    let assets = root.join("assets");
    directory(&assets)?;
    let mut asset_changed = false;
    for asset in &manifest.assets {
        let destination = assets.join(&asset.hash);
        if read_file(&destination, asset.length).is_ok_and(|bytes| asset.matches(&bytes)) {
            continue;
        }
        let source = link_root.join("assets").join(&asset.hash);
        if let Ok(bytes) = read_file(&source, asset.length)
            && asset.matches(&bytes)
        {
            atomic_write(&destination, &bytes)?;
            asset_changed = true;
        }
    }
    if content_changed {
        atomic_write(&root.join("manifest.json"), &bytes)?;
    }
    // Pending WatchConnectivity transfers own separate immutable staging copies.
    for file in fs::read_dir(&assets).map_err(failure)?.flatten() {
        if !manifest
            .assets
            .iter()
            .any(|asset| file.file_name() == asset.hash.as_str())
        {
            let _ = fs::remove_file(file.path());
        }
    }
    Ok(content_changed || asset_changed)
}

#[cfg(test)]
#[path = "watch_outbox_test.rs"]
mod watch_outbox_test;
