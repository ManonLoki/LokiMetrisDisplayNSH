//! 只读查询共享连接的内容寻址资源目录。
// 引入资源描述
use loki_metris_pairing::assets::AssetDescriptor;
// 引入文件系统与读取
use std::{
    // 文件操作、文件与元数据类型
    fs::{self, File, Metadata},
    // 读特征
    io::Read,
    // 路径类型
    path::Path,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
// 文件身份：Unix 上由设备号与 inode 标识
struct FileIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

// 取文件身份，非 Unix 平台无法取得
fn identity(metadata: &Metadata) -> Option<FileIdentity> {
    #[cfg(unix)]
    {
        // Unix 元数据扩展
        use std::os::unix::fs::MetadataExt;
        // 构造文件身份
        Some(FileIdentity {
            // 设备号
            device: metadata.dev(),
            // 索引节点号
            inode: metadata.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        // 忽略元数据
        let _ = metadata;
        // 本平台不做缓存
        None
    }
}

// 必须是普通文件、不是符号链接且长度与描述一致
fn regular(metadata: &Metadata, descriptor: &AssetDescriptor) -> bool {
    // 普通文件、非链接且长度与描述一致
    metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() == descriptor.length
}

/// 目录或叶子节点被改动后,不能把缓存的图片重定向到别的文件。
// 目录必须是真实目录而非符号链接，避免被换成别处
fn asset_directory(path: &Path) -> Result<(), String> {
    // 读取元数据，不跟随链接
    let metadata = fs::symlink_metadata(path).map_err(super::err)?;
    // 不是目录或是链接
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        // 图片缓存目录无效
        return Err("图片缓存目录无效".into());
    }
    // 完成
    Ok(())
}

// 两次元数据是否指向同一文件：优先比身份，否则比长度与修改时间
fn same_file(before: &Metadata, after: &Metadata) -> bool {
    // 比较打开前后的文件身份
    match (identity(before), identity(after)) {
        // 两者一致
        (Some(before), Some(after)) => before == after,
        // 长度与修改时间都未变
        _ => before.len() == after.len() && before.modified().ok() == after.modified().ok(),
    }
}

/// 通过同一个经检查的文件句柄,验证长度、格式和摘要。
// 通过同一个已检查的文件句柄校验长度、格式与摘要
pub(super) fn verify_file(path: &Path, descriptor: &AssetDescriptor) -> Result<(), String> {
    // 先校验描述本身
    descriptor.validate().map_err(super::err)?;
    // 再校验目录
    asset_directory(path.parent().ok_or("图片缓存目录无效")?)?;
    // 打开前的元数据
    let before = fs::symlink_metadata(path).map_err(super::err)?;
    // 不是合格的普通文件
    if !regular(&before, descriptor) {
        // 缓存文件无效
        return Err("图片缓存文件无效".into());
    }
    // 打开文件
    let file = File::open(path).map_err(super::err)?;
    // 打开后再检查一遍，防止中间被替换
    let opened = file.metadata().map_err(super::err)?;
    // 打开后的文件不合格或与之前不是同一个
    if !regular(&opened, descriptor) || !same_file(&before, &opened) {
        // 缓存文件已变化
        return Err("图片缓存文件已变化".into());
    }
    // 最多读取比声明多一字节，超长即可发现
    let mut bytes = Vec::new();
    // 最多读取描述长度加一字节
    file.take(descriptor.length + 1)
        // 读入缓冲区
        .read_to_end(&mut bytes)
        // 失败转为错误
        .map_err(super::err)?;
    // 读完后第三次检查
    let after = fs::symlink_metadata(path).map_err(super::err)?;
    // 校验和也必须匹配
    if !regular(&after, descriptor) || !same_file(&opened, &after) || !descriptor.matches(&bytes) {
        // 校验失败
        return Err("图片缓存校验失败".into());
    }
    // 完成
    Ok(())
}

// 测试模块
#[cfg(test)]
#[path = "asset_cache_test.rs"]
mod asset_cache_test;
