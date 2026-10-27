//! Immutable, verified private file copies kept until the OS completes or cancels delivery.
use loki_metris_pairing::assets::AssetDescriptor;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(crate) fn stage(root: &Path, descriptor: &AssetDescriptor) -> Result<PathBuf, String> {
    descriptor.validate().map_err(|_| "图片描述无效")?;
    let source = root.join("assets").join(&descriptor.hash);
    let metadata = fs::symlink_metadata(&source).map_err(|_| "图片不可用")?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != descriptor.length
    {
        return Err("图片不可用".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(source)
        .map_err(|_| "图片不可用")?
        .take(descriptor.length + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "图片不可用")?;
    if !descriptor.matches(&bytes) {
        return Err("图片校验失败".into());
    }
    let staging = root.join("staging");
    fs::create_dir_all(&staging).map_err(|_| "传输目录不可用")?;
    let meta = fs::symlink_metadata(&staging).map_err(|_| "传输目录不可用")?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("传输目录无效".into());
    }
    let path = staging.join(format!("asset-{}", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = options.open(&path).and_then(|mut file| {
        file.write_all(&bytes)?;
        file.sync_all()
    });
    if result.is_err() {
        remove(root, &path);
        return Err("图片暂存失败".into());
    }
    Ok(path)
}

pub(crate) fn remove(root: &Path, path: &Path) {
    if path.parent() != Some(root.join("staging").as_path()) {
        return;
    }
    let Some(name) = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("asset-"))
    else {
        return;
    };
    if uuid::Uuid::parse_str(name).is_ok_and(|id| id.to_string() == name) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
#[path = "files_test.rs"]
mod tests;
