//! Fail-closed validation before Android opens or creates its vault data key.
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{fs, io::ErrorKind, path::Path};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Envelope {
    New,
    Existing { iv: [u8; 12], sealed: Vec<u8> },
}

pub(super) fn read(
    iv: Option<&str>,
    wrapped: Option<&str>,
    vault: &Path,
) -> Result<Envelope, &'static str> {
    match (iv, wrapped) {
        (Some(iv), Some(wrapped)) => {
            let iv = STANDARD
                .decode(iv)
                .ok()
                .and_then(|bytes| bytes.try_into().ok())
                .ok_or("Android 安全存储密钥记录损坏。")?;
            let sealed = STANDARD
                .decode(wrapped)
                .map_err(|_| "Android 安全存储密钥记录损坏。")?;
            if sealed.len() != 32 + 16 {
                return Err("Android 安全存储密钥记录损坏。");
            }
            Ok(Envelope::Existing { iv, sealed })
        }
        (None, None) => {
            match fs::read_dir(vault) {
                Ok(mut entries) => {
                    if entries.next().is_some() {
                        return Err("Android 安全存储密钥记录缺失，已有配对记录已保留。");
                    }
                }
                Err(error) if error.kind() != ErrorKind::NotFound => {
                    return Err("Android 安全存储目录不可读，已有数据已保留。");
                }
                _ => {}
            }
            Ok(Envelope::New)
        }
        _ => Err("Android 安全存储密钥记录不完整，已有数据已保留。"),
    }
}

#[cfg(test)]
#[path = "android_key_record_test.rs"]
mod tests;
