//! Only bounded inline GIFs and regular, non-symlink local files reach the decoder.
use super::MAX_SOURCE;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn file_path(source: &str) -> Result<PathBuf, String> {
    let raw = source
        .strip_prefix("file://")
        .filter(|path| path.starts_with('/'))
        .ok_or("图片来源必须是本地file:///或内联GIF")?;
    if raw.contains(['?', '#']) {
        return Err("图片文件URL不能包含查询或片段".into());
    }
    let mut bytes = Vec::with_capacity(raw.len());
    let mut input = raw.bytes();
    while let Some(byte) = input.next() {
        let value = if byte == b'%' {
            let high = input.next().and_then(hex).ok_or("图片URL转义无效")?;
            let low = input.next().and_then(hex).ok_or("图片URL转义无效")?;
            high * 16 + low
        } else {
            byte
        };
        if value == 0 {
            return Err("图片URL包含空字符".into());
        }
        bytes.push(value);
    }
    let text = String::from_utf8(bytes).map_err(|_| "图片URL不是UTF-8")?;
    #[cfg(windows)]
    let text = if text.as_bytes().get(2) == Some(&b':') {
        text[1..].into()
    } else {
        text
    };
    let path = PathBuf::from(text);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("图片文件路径无效".into());
    }
    Ok(platform_path(path))
}

fn platform_path(path: PathBuf) -> PathBuf {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    for (alias, expected) in [("/var", "/private/var"), ("/tmp", "/private/tmp")] {
        // Apple's sandbox/temp URLs use these fixed system aliases. Resolve
        // only this known root mapping, never arbitrary application symlinks.
        if let Ok(rest) = path.strip_prefix(alias)
            && let Ok(target) = fs::read_link(alias)
        {
            let target = if target.is_absolute() {
                target
            } else {
                Path::new("/").join(target)
            };
            if target == Path::new(expected) {
                return Path::new(expected).join(rest);
            }
        }
    }
    path
}

fn regular_path(path: &Path) -> Result<fs::Metadata, String> {
    let mut checked = PathBuf::new();
    for component in path.components() {
        checked.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&checked).map_err(|_| "图片文件不可读取")?;
        if metadata.file_type().is_symlink() {
            return Err("图片路径不能使用符号链接".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("图片路径不能使用重解析链接".into());
            }
        }
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| "图片文件不可读取")?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE as u64 {
        return Err("图片文件不是普通文件或超过限额".into());
    }
    Ok(metadata)
}

pub(super) fn load(source: &str) -> Result<(Vec<u8>, bool), String> {
    if let Some(encoded) = source.strip_prefix("data:image/gif;base64,") {
        if encoded.len() > MAX_SOURCE.div_ceil(3) * 4 {
            return Err("GIF来源超过长度限额".into());
        }
        let bytes = STANDARD.decode(encoded).map_err(|_| "GIF Base64编码无效")?;
        if bytes.len() > MAX_SOURCE {
            return Err("GIF来源超过长度限额".into());
        }
        return Ok((bytes, true));
    }
    if source.len() > 32 * 1024 {
        return Err("图片URL超过长度限额".into());
    }
    let path = file_path(source)?;
    let before = regular_path(&path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000); // O_NOFOLLOW: do not open a replacement symlink.
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x100); // Darwin O_NOFOLLOW.
    }
    let file = options.open(&path).map_err(|_| "图片文件不能安全打开")?;
    let opened = file.metadata().map_err(|_| "图片文件信息不可读取")?;
    if !opened.is_file() || opened.len() > MAX_SOURCE as u64 {
        return Err("图片文件不是普通文件或超过限额".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != opened.dev() || before.ino() != opened.ino() {
            return Err("图片文件在读取时发生变化".into());
        }
    }
    #[cfg(not(unix))]
    if before.len() != opened.len() || before.modified().ok() != opened.modified().ok() {
        return Err("图片文件在读取时发生变化".into());
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    file.take(MAX_SOURCE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "图片读取失败")?;
    regular_path(&path)?;
    if bytes.len() > MAX_SOURCE {
        return Err("图片来源超过长度限额".into());
    }
    Ok((bytes, false))
}

#[cfg(test)]
#[path = "source_test.rs"]
mod source_test;
