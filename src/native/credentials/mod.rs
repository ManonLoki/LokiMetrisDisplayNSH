//! 配对记录的系统安全存储。秘密不进入 JS 存储或命令行参数；存储不可用或锁定时失败关闭。

pub fn read(service: &str, account: &str) -> Result<Option<String>, String> {
    validate_key(service, account)?;
    platform::read(service, account)
}

pub fn write(service: &str, account: &str, value: &str) -> Result<(), String> {
    validate_key(service, account)?;
    platform::write(service, account, value)
}

pub fn delete(service: &str, account: &str) -> Result<(), String> {
    validate_key(service, account)?;
    platform::delete(service, account)
}

fn validate_key(service: &str, account: &str) -> Result<(), String> {
    if [service, account]
        .iter()
        .any(|s| s.is_empty() || s.len() > 512 || s.chars().any(char::is_control))
    {
        return Err("无效的凭据标识".into());
    }
    Ok(())
}

/// Android 宿主交给本进程的 Keystore 包裹密钥；其他平台无需配置。
#[cfg(target_os = "android")]
pub fn configure_vault(dir: std::path::PathBuf, key: [u8; 32]) -> Result<(), String> {
    platform::configure(dir, key)
}

/// 只添加缺失项（iOS 本机身份）；已存在返回 false。
#[cfg(target_os = "ios")]
pub fn add_only(service: &str, account: &str, value: &str) -> Result<bool, String> {
    validate_key(service, account)?;
    platform::add_only(service, account, value)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[path = "macos.rs"]
mod platform;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;

#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

#[cfg(target_os = "android")]
#[path = "android.rs"]
mod platform;

#[cfg(test)]
#[path = "credentials_test.rs"]
mod credentials_test;
