//! 不可变且已验证的私有文件副本,保留到操作系统完成或取消投递为止。
// 引入资源描述
use loki_metris_pairing::assets::AssetDescriptor;
// 引入文件系统与路径
use std::{
    // 文件操作与打开选项
    fs::{self, OpenOptions},
    // 读写特征
    io::{Read, Write},
    // 路径类型
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
    // 不是普通文件
    if !metadata.is_file()
        // 或者是符号链接
        || metadata.file_type().is_symlink()
        // 或者长度与描述不符
        || metadata.len() != descriptor.length
    {
        // 图片不可用
        return Err("图片不可用".into());
    }
    // 读取并校验摘要
    let mut bytes = Vec::new();
    // 打开源文件
    fs::File::open(source)
        // 失败报图片不可用
        .map_err(|_| "图片不可用")?
        // 最多读描述长度加一字节
        .take(descriptor.length + 1)
        // 读入缓冲区
        .read_to_end(&mut bytes)
        // 失败报图片不可用
        .map_err(|_| "图片不可用")?;
    // 校验失败
    if !descriptor.matches(&bytes) {
        // 报图片校验失败
        return Err("图片校验失败".into());
    }
    // 暂存目录必须是真实目录
    let staging = root.join("staging");
    // 创建暂存目录
    fs::create_dir_all(&staging).map_err(|_| "传输目录不可用")?;
    // 读取元数据，不跟随链接
    let meta = fs::symlink_metadata(&staging).map_err(|_| "传输目录不可用")?;
    // 不是目录或是链接
    if !meta.is_dir() || meta.file_type().is_symlink() {
        // 暂存目录无效
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
    // 写入并同步到磁盘
    let result = options.open(&path).and_then(|mut file| {
        // 写入全部字节
        file.write_all(&bytes)?;
        // 刷盘
        file.sync_all()
    });
    // 失败则清理残留
    if result.is_err() {
        // 删除已写的文件
        remove(root, &path);
        // 图片暂存失败
        return Err("图片暂存失败".into());
    }
    // 返回暂存路径
    Ok(path)
}

// 删除暂存文件：只接受 staging 目录下形如 asset-UUID 的文件名
pub(crate) fn remove(root: &Path, path: &Path) {
    // 文件不在暂存目录内
    if path.parent() != Some(root.join("staging").as_path()) {
        // 直接返回
        return;
    }
    // 取文件名
    let Some(name) = path
        // 文件名
        .file_name()
        // 转为字符串
        .and_then(|name| name.to_str())
        // 须以 asset- 开头
        .and_then(|name| name.strip_prefix("asset-"))
    // 不满足条件时
    else {
        // 直接返回
        return;
    };
    // 文件名必须是规范 UUID
    if uuid::Uuid::parse_str(name).is_ok_and(|id| id.to_string() == name) {
        // 删除文件，失败忽略
        let _ = fs::remove_file(path);
    }
}

// 测试模块
#[cfg(test)]
#[path = "files_test.rs"]
mod tests;
