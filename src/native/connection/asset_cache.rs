//! Read-only queries share the connection's content-addressed asset directory.
use loki_metris_pairing::assets::AssetDescriptor;
use std::{
    fs::{self, File, Metadata},
    io::Read,
    path::Path,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

fn identity(metadata: &Metadata) -> Option<FileIdentity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(FileIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

fn regular(metadata: &Metadata, descriptor: &AssetDescriptor) -> bool {
    metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() == descriptor.length
}

/// A changed directory or leaf cannot redirect a cached image to another file.
fn asset_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(super::err)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("图片缓存目录无效".into());
    }
    Ok(())
}

fn same_file(before: &Metadata, after: &Metadata) -> bool {
    match (identity(before), identity(after)) {
        (Some(before), Some(after)) => before == after,
        _ => before.len() == after.len() && before.modified().ok() == after.modified().ok(),
    }
}

/// Verify length, format and digest through the same checked file handle.
pub(super) fn verify_file(path: &Path, descriptor: &AssetDescriptor) -> Result<(), String> {
    descriptor.validate().map_err(super::err)?;
    asset_directory(path.parent().ok_or("图片缓存目录无效")?)?;
    let before = fs::symlink_metadata(path).map_err(super::err)?;
    if !regular(&before, descriptor) {
        return Err("图片缓存文件无效".into());
    }
    let file = File::open(path).map_err(super::err)?;
    let opened = file.metadata().map_err(super::err)?;
    if !regular(&opened, descriptor) || !same_file(&before, &opened) {
        return Err("图片缓存文件已变化".into());
    }
    let mut bytes = Vec::new();
    file.take(descriptor.length + 1)
        .read_to_end(&mut bytes)
        .map_err(super::err)?;
    let after = fs::symlink_metadata(path).map_err(super::err)?;
    if !regular(&after, descriptor) || !same_file(&opened, &after) || !descriptor.matches(&bytes) {
        return Err("图片缓存校验失败".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "asset_cache_test.rs"]
mod asset_cache_test;
