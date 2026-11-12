//! 只有有界的内联 GIF 以及普通的、非符号链接的本地文件才能到达解码器。
use super::MAX_SOURCE;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

// 十六进制字符转数值
fn hex(byte: u8) -> Option<u8> {
    // 按字节取十六进制值
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

// 把 file:// URL 转为本地路径，拒绝查询、片段、空字符与 ..
fn file_path(source: &str) -> Result<PathBuf, String> {
    // 必须是 file:/// 开头
    let raw = source
        // 去掉 file:// 前缀
        .strip_prefix("file://")
        .filter(|path| path.starts_with('/'))
        // 否则报来源必须是本地 file:/// 或内联 GIF
        .ok_or("图片来源必须是本地file:///或内联GIF")?;
    if raw.contains(['?', '#']) {
        // 报不能包含查询或片段
        return Err("图片文件URL不能包含查询或片段".into());
    }
    // 逐字节解码百分号转义
    let mut bytes = Vec::with_capacity(raw.len());
    let mut input = raw.bytes();
    while let Some(byte) = input.next() {
        let value = if byte == b'%' {
            let high = input.next().and_then(hex).ok_or("图片URL转义无效")?;
            let low = input.next().and_then(hex).ok_or("图片URL转义无效")?;
            high * 16 + low
        // 否则分支
        } else {
            byte
        };
        if value == 0 {
            return Err("图片URL包含空字符".into());
        }
        bytes.push(value);
    }
    // 转为字符串，非 UTF-8 则报错
    let text = String::from_utf8(bytes).map_err(|_| "图片URL不是UTF-8")?;
    // Windows：去掉盘符前多余的斜杠
    #[cfg(windows)]
    let text = if text.as_bytes().get(2) == Some(&b':') {
        text[1..].into()
    // 否则分支
    } else {
        text
    };
    let path = PathBuf::from(text);
    // 必须是绝对路径且不含 ..
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("图片文件路径无效".into());
    }
    Ok(platform_path(path))
}

// 平台相关的路径修正
fn platform_path(path: PathBuf) -> PathBuf {
    // Apple 的沙盒与临时目录 URL 使用固定的系统别名；只解析这两个已知的根映射，绝不解析应用内任意符号链接
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    for (alias, expected) in [("/var", "/private/var"), ("/tmp", "/private/tmp")] {
        // Apple 的沙盒/临时 URL 使用这些固定的系统别名。只解析
        // 这个已知的根映射,绝不解析任意的应用符号链接。
        if let Ok(rest) = path.strip_prefix(alias)
            // 并且能读取别名的链接目标
            && let Ok(target) = fs::read_link(alias)
        {
            let target = if target.is_absolute() {
                target
            // 否则
            } else {
                Path::new("/").join(target)
            };
            if target == Path::new(expected) {
                // 返回前缀加剩余部分
                return Path::new(expected).join(rest);
            }
        }
    }
    path
}

// 逐级检查路径：任一级为符号链接即拒绝，最终必须是不超限的普通文件
fn regular_path(path: &Path) -> Result<fs::Metadata, String> {
    let mut checked = PathBuf::new();
    for component in path.components() {
        checked.push(component.as_os_str());
        // 读取元数据，不跟随链接
        let metadata = fs::symlink_metadata(&checked).map_err(|_| "图片文件不可读取")?;
        if metadata.file_type().is_symlink() {
            // 报不能使用符号链接
            return Err(format!("图片路径不能使用符号链接: {}", checked.display()));
        }
        // Windows 还要拒绝重解析点
        #[cfg(windows)]
        {
            // Windows 元数据扩展
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                // 报不能使用重解析链接
                return Err("图片路径不能使用重解析链接".into());
            }
        }
    }
    // 读取元数据，不跟随链接
    let metadata = fs::symlink_metadata(path).map_err(|_| "图片文件不可读取")?;
    // 必须是普通文件且不超限
    if !metadata.is_file() || metadata.len() > MAX_SOURCE as u64 {
        // 否则报错
        return Err("图片文件不是普通文件或超过限额".into());
    }
    Ok(metadata)
}

// 读取来源：内联 GIF 或本地普通文件；返回字节和是否为内联 GIF
pub(super) fn load(source: &str) -> Result<(Vec<u8>, bool), String> {
    // 内联 GIF：先限制编码长度，再解码并再次限制
    if let Some(encoded) = source.strip_prefix("data:image/gif;base64,") {
        if encoded.len() > MAX_SOURCE.div_ceil(3) * 4 {
            return Err("GIF来源超过长度限额".into());
        }
        // Base64 解码
        let bytes = STANDARD.decode(encoded).map_err(|_| "GIF Base64编码无效")?;
        if bytes.len() > MAX_SOURCE {
            return Err("GIF来源超过长度限额".into());
        }
        // 返回字节与来源标记
        return Ok((bytes, true));
    }
    if source.len() > 32 * 1024 {
        return Err("图片URL超过长度限额".into());
    }
    let path = file_path(source)?;
    let before = regular_path(&path)?;
    let mut options = fs::OpenOptions::new();
    // 只读
    options.read(true);
    // 使用 O_NOFOLLOW，不打开被替换成符号链接的文件
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // Unix 打开选项扩展
        use std::os::unix::fs::OpenOptionsExt;
        // 打开时不跟随符号链接（Linux）
        options.custom_flags(0x20000); // O_NOFOLLOW:不要打开被替换成符号链接的文件。
    }
    // Darwin 的 O_NOFOLLOW
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        // Unix 打开选项扩展
        use std::os::unix::fs::OpenOptionsExt;
        // 打开时不跟随符号链接（Darwin）
        options.custom_flags(0x100); // Darwin 的 O_NOFOLLOW。
    }
    let file = options.open(&path).map_err(|_| "图片文件不能安全打开")?;
    // 打开后再检查一遍
    let opened = file.metadata().map_err(|_| "图片文件信息不可读取")?;
    // 不是普通文件或超限
    if !opened.is_file() || opened.len() > MAX_SOURCE as u64 {
        return Err("图片文件不是普通文件或超过限额".into());
    }
    // Unix 上比较设备号与 inode，防止检查与打开之间被替换
    #[cfg(unix)]
    {
        // Unix 元数据扩展
        use std::os::unix::fs::MetadataExt;
        // 设备或 inode 变了
        if before.dev() != opened.dev() || before.ino() != opened.ino() {
            // 报文件在读取时变化
            return Err("图片文件在读取时发生变化".into());
        }
    }
    // 其他平台比较长度与修改时间
    #[cfg(not(unix))]
    if before.len() != opened.len() || before.modified().ok() != opened.modified().ok() {
        // 报文件在读取时变化
        return Err("图片文件在读取时发生变化".into());
    }
    // 最多读取比上限多一字节
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    // 最多读限额加一字节
    file.take(MAX_SOURCE as u64 + 1)
        .read_to_end(&mut bytes)
        // 失败报图片读取失败
        .map_err(|_| "图片读取失败")?;
    // 读完后再检查一次路径
    regular_path(&path)?;
    if bytes.len() > MAX_SOURCE {
        return Err("图片来源超过长度限额".into());
    }
    // 返回字节，标记非内联数据
    Ok((bytes, false))
}

#[cfg(test)]
#[path = "source_test.rs"]
mod source_test;
