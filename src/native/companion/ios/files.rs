//! 不可变且已验证的私有文件副本,保留到操作系统完成或取消投递为止。
use loki_metris_pairing::assets::AssetDescriptor;
use std::{
    // 文件操作与打开选项
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

// 暂存一份不可变、已校验的私有文件副本，直到系统完成或取消传输
pub(crate) fn stage(root: &Path, descriptor: &AssetDescriptor) -> Result<PathBuf, String> {
    // 先校验描述
    descriptor.validate().map_err(|_| "图片描述无效")?;
    // 来源是缓存目录中按摘要命名的文件
    let source = root.join("assets").join(&descriptor.hash);
    // 必须是普通文件、非符号链接且长度一致
    let metadata = fs::symlink_metadata(&source).map_err(|_| "图片不可用")?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        // 或者长度与描述不符
        || metadata.len() != descriptor.length
    {
        return Err("图片不可用".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(source)
        .map_err(|_| "图片不可用")?
        // 最多读描述长度加一字节
        .take(descriptor.length + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "图片不可用")?;
    if !descriptor.matches(&bytes) {
        return Err("图片校验失败".into());
    }
    // 暂存目录必须是真实目录
    let staging = root.join("staging");
    fs::create_dir_all(&staging).map_err(|_| "传输目录不可用")?;
    // 读取元数据，不跟随链接
    let meta = fs::symlink_metadata(&staging).map_err(|_| "传输目录不可用")?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("传输目录无效".into());
    }
    // 随机名称，避免与其他传输冲突
    let path = staging.join(format!("asset-{}", uuid::Uuid::new_v4()));
    // 只创建新文件，Unix 上权限为 0600
    let mut options = OpenOptions::new();
    // 可写且必须新建
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        // Unix 打开选项扩展
        use std::os::unix::fs::OpenOptionsExt;
        // 仅所有者可读写
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

// 删除暂存文件：只接受 staging 目录下形如 asset-UUID 的文件名
pub(crate) fn remove(root: &Path, path: &Path) {
    // 文件不在暂存目录内
    if path.parent() != Some(root.join("staging").as_path()) {
        return;
    }
    let Some(name) = path
        .file_name()
        .and_then(|name| name.to_str())
        // 须以 asset- 开头
        .and_then(|name| name.strip_prefix("asset-"))
    else {
        return;
    };
    // 文件名必须是规范 UUID
    if uuid::Uuid::parse_str(name).is_ok_and(|id| id.to_string() == name) {
        // 删除文件，失败忽略
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
#[path = "files_test.rs"]
mod tests;
