//! 在 Android 打开或创建保险库数据密钥之前,执行失败即关闭的校验。
// 引入 base64
use base64::{Engine, engine::general_purpose::STANDARD};
// 引入文件系统与路径
use std::{fs, io::ErrorKind, path::Path};

#[derive(Debug, PartialEq, Eq)]
// 密钥记录的状态
pub(super) enum Envelope {
    // 全新安装，需要创建密钥
    New,
    // 已有记录：IV 与被包裹的密钥
    Existing { iv: [u8; 12], sealed: Vec<u8> },
}

// 在 Android 打开或创建保险库密钥之前做失败关闭的校验
pub(super) fn read(
    // 初始向量
    iv: Option<&str>,
    // 包装密钥
    wrapped: Option<&str>,
    // 保险库目录
    vault: &Path,
    // 返回信封
) -> Result<Envelope, &'static str> {
    // 按初始向量与包装密钥是否存在分类
    match (iv, wrapped) {
        // 两项都在：解码并检查长度
        (Some(iv), Some(wrapped)) => {
            // IV 必须是 12 字节
            let iv = STANDARD
                // 解码初始向量
                .decode(iv)
                // 失败为 None
                .ok()
                // 转为定长数组
                .and_then(|bytes| bytes.try_into().ok())
                // 失败报记录损坏
                .ok_or("Android 安全存储密钥记录损坏。")?;
            // 被包裹密钥必须是 32 字节密钥加 16 字节标签
            let sealed = STANDARD
                // 解码包装密钥
                .decode(wrapped)
                // 失败报记录损坏
                .map_err(|_| "Android 安全存储密钥记录损坏。")?;
            // 密文长度须为 48
            if sealed.len() != 32 + 16 {
                // 密钥记录损坏
                return Err("Android 安全存储密钥记录损坏。");
            }
            // 返回已有信封
            Ok(Envelope::Existing { iv, sealed })
        }
        // 两项都没有：只有保险库目录为空才允许新建，否则保留既有数据并报错
        (None, None) => {
            // 读取保险库目录
            match fs::read_dir(vault) {
                // 读取成功
                Ok(mut entries) => {
                    // 目录非空
                    if entries.next().is_some() {
                        // 密钥记录缺失，保留已有配对
                        return Err("Android 安全存储密钥记录缺失，已有配对记录已保留。");
                    }
                }
                // 不是“不存在”的错误
                Err(error) if error.kind() != ErrorKind::NotFound => {
                    // 目录不可读，保留已有数据并报错
                    return Err("Android 安全存储目录不可读，已有数据已保留。");
                }
                // 其余情形不处理
                _ => {}
            }
            // 没有旧记录：创建新信封
            Ok(Envelope::New)
        }
        // 只有一项在：记录不完整
        _ => Err("Android 安全存储密钥记录不完整，已有数据已保留。"),
    }
}

// 测试模块
#[cfg(test)]
#[path = "key_record_test.rs"]
mod tests;
