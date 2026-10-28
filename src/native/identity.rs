//! 系统凭据存储中的持久设备身份,独立于配对记录。
// 非 iOS 目标不使用其中部分函数
#![cfg_attr(not(target_os = "ios"), allow(dead_code))]

#[derive(Debug, PartialEq, Eq)]
// 身份读取或保存的失败原因
pub(crate) enum Failure {
    // 身份无效或记录损坏
    InvalidIdentity,
    // 另一个启动请求先保存了不同身份
    ConflictingIdentity,
    // 安全存储不可用
    StoreUnavailable,
    // 写入失败
    StoreWriteFailed,
}

// 失败类型的方法
impl Failure {
    // 面向用户的提示文案
    pub(crate) fn message(&self) -> &'static str {
        // 按失败种类返回提示
        match self {
            // 无法访问钥匙串
            Self::StoreUnavailable => "无法访问本机身份钥匙串。请解锁设备并重新打开应用。",
            // 保存失败的提示
            Self::StoreWriteFailed => "未能保存设备身份。请检查系统安全存储后重试。",
            // 身份冲突的提示
            Self::ConflictingIdentity => "本机身份已被另一启动请求保存，请重新打开应用。",
            // 记录损坏的提示
            Self::InvalidIdentity => "设备身份记录损坏，请检查本机安全存储。",
        }
    }
}

// 读取已保存的身份，没有则生成并保存；读写通过闭包注入，便于测试
pub(crate) fn load(
    // 读取已有身份
    read: impl Fn() -> Result<Option<String>, Failure>,
    // 保存新身份
    save: impl FnOnce(&str) -> Result<bool, Failure>,
    // 生成新身份
    generate: impl FnOnce() -> String,
    // 返回身份
) -> Result<String, Failure> {
    // 设备标识校验
    use crate::features::connection::identity::valid_device_id;
    // 已有身份必须合法，否则报告损坏
    if let Some(saved) = read()? {
        // 已保存的标识合法
        return if valid_device_id(&saved) {
            // 沿用
            Ok(saved)
        // 否则分支
        } else {
            // 否则报身份无效
            Err(Failure::InvalidIdentity)
        };
    }
    // 没有则生成
    let identity = generate();
    // 生成结果也要合法
    if !valid_device_id(&identity) {
        // 身份无效
        return Err(Failure::InvalidIdentity);
    }
    // 保存时若已存在，只有读回的就是自己才算成功
    if !save(&identity)? && read()?.as_deref() != Some(identity.as_str()) {
        // 身份冲突
        return Err(Failure::ConflictingIdentity);
    }
    // 返回身份
    Ok(identity)
}

#[cfg(target_os = "ios")]
// iOS：在系统钥匙串中持久化设备身份
pub(crate) fn device_identity(service: &str) -> Result<String, Failure> {
    // 凭据读写
    use crate::native::credentials;
    // 钥匙串账户名
    const ACCOUNT: &str = "device-id";
    // 读取、仅添加缺失项、用随机 UUID 生成
    load(
        // 读取系统凭据，失败报存储不可用
        || credentials::read(service, ACCOUNT).map_err(|_| Failure::StoreUnavailable),
        // 保存回调
        |identity| {
            // 仅在不存在时添加，失败报写入失败
            credentials::add_only(service, ACCOUNT, identity).map_err(|_| Failure::StoreWriteFailed)
        },
        // 生成新的 UUID
        || uuid::Uuid::new_v4().to_string(),
    )
}

// 测试模块
#[cfg(test)]
#[path = "identity_test.rs"]
mod identity_test;
