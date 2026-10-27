//! Persistent device identity in the system credential store, independent of pairing records.
#![cfg_attr(not(target_os = "ios"), allow(dead_code))]

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    InvalidIdentity,
    ConflictingIdentity,
    StoreUnavailable,
    StoreWriteFailed,
}

impl Failure {
    pub(crate) fn message(&self) -> &'static str {
        match self {
            Self::StoreUnavailable => "无法访问本机身份钥匙串。请解锁设备并重新打开应用。",
            Self::StoreWriteFailed => "未能保存设备身份。请检查系统安全存储后重试。",
            Self::ConflictingIdentity => "本机身份已被另一启动请求保存，请重新打开应用。",
            Self::InvalidIdentity => "设备身份记录损坏，请检查本机安全存储。",
        }
    }
}

pub(crate) fn load(
    read: impl Fn() -> Result<Option<String>, Failure>,
    save: impl FnOnce(&str) -> Result<bool, Failure>,
    generate: impl FnOnce() -> String,
) -> Result<String, Failure> {
    use crate::features::connection::identity::valid_device_id;
    if let Some(saved) = read()? {
        return if valid_device_id(&saved) {
            Ok(saved)
        } else {
            Err(Failure::InvalidIdentity)
        };
    }
    let identity = generate();
    if !valid_device_id(&identity) {
        return Err(Failure::InvalidIdentity);
    }
    if !save(&identity)? && read()?.as_deref() != Some(identity.as_str()) {
        return Err(Failure::ConflictingIdentity);
    }
    Ok(identity)
}

#[cfg(target_os = "ios")]
pub(crate) fn device_identity(service: &str) -> Result<String, Failure> {
    use crate::native::credentials;
    const ACCOUNT: &str = "device-id";
    load(
        || credentials::read(service, ACCOUNT).map_err(|_| Failure::StoreUnavailable),
        |identity| {
            credentials::add_only(service, ACCOUNT, identity).map_err(|_| Failure::StoreWriteFailed)
        },
        || uuid::Uuid::new_v4().to_string(),
    )
}

#[cfg(test)]
#[path = "identity_test.rs"]
mod identity_test;
