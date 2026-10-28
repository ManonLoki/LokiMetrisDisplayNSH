//! 配对记录的系统安全存储。秘密不进入 JS 存储或命令行参数；存储不可用或锁定时失败关闭。

// 读取凭据；标识不合法或存储不可用时失败
pub fn read(service: &str, account: &str) -> Result<Option<String>, String> {
    // 先校验标识
    validate_key(service, account)?;
    // 交给平台读取
    platform::read(service, account)
}

// 写入凭据
pub fn write(service: &str, account: &str, value: &str) -> Result<(), String> {
    // 先校验标识
    validate_key(service, account)?;
    // 交给平台写入
    platform::write(service, account, value)
}

// 删除凭据
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    // 先校验标识
    validate_key(service, account)?;
    // 交给平台删除
    platform::delete(service, account)
}

// 标识必须非空、不超过 512 字节且不含控制字符
fn validate_key(service: &str, account: &str) -> Result<(), String> {
    // 服务名与账户名不得含非法字符
    if [service, account]
        // 遍历
        .iter()
        // 为空、过长或含控制字符
        .any(|s| s.is_empty() || s.len() > 512 || s.chars().any(char::is_control))
    {
        // 标识无效
        return Err("无效的凭据标识".into());
    }
    // 合法
    Ok(())
}

/// Android 宿主交给本进程的 Keystore 包裹密钥；其他平台无需配置。
// Android：配置 Keystore 包裹密钥
#[cfg(target_os = "android")]
pub fn configure_vault(dir: std::path::PathBuf, key: [u8; 32]) -> Result<(), String> {
    // 交给平台配置
    platform::configure(dir, key)
}

/// 只添加缺失项（iOS 本机身份）；已存在返回 false。
// iOS：只添加缺失项
#[cfg(target_os = "ios")]
pub fn add_only(service: &str, account: &str, value: &str) -> Result<bool, String> {
    // 先校验标识
    validate_key(service, account)?;
    // 仅在不存在时添加
    platform::add_only(service, account, value)
}

// 各平台的实现：macOS/iOS 钥匙串、Linux、Windows、Android
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[path = "macos.rs"]
mod platform;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
// Linux 平台实现
mod platform;

#[cfg(target_os = "windows")]
#[path = "windows.rs"]
// Windows 平台实现
mod platform;

#[cfg(target_os = "android")]
#[path = "android.rs"]
// 其他平台实现
mod platform;

// 测试模块
#[cfg(test)]
#[path = "credentials_test.rs"]
mod credentials_test;
