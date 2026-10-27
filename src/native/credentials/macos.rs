//! Apple Keychain 的所有权与返回码只在此处处理，调用方不接触原生指针。
use std::{ffi::c_void, ptr};

type CF = *const c_void;
const UTF8: u32 = 0x0800_0100;
const NOT_FOUND: i32 = -25300;
#[cfg(target_os = "ios")]
const DUPLICATE: i32 = -25299;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithBytes(
        allocator: CF,
        bytes: *const u8,
        length: isize,
        encoding: u32,
        external: u8,
    ) -> CF;
    fn CFDataCreate(allocator: CF, bytes: *const u8, length: isize) -> CF;
    fn CFDataGetLength(data: CF) -> isize;
    fn CFDataGetBytePtr(data: CF) -> *const u8;
    fn CFGetTypeID(value: CF) -> usize;
    fn CFDataGetTypeID() -> usize;
    fn CFDictionaryCreate(
        allocator: CF,
        keys: *const CF,
        values: *const CF,
        count: isize,
        key_callbacks: CF,
        value_callbacks: CF,
    ) -> CF;
    fn CFRelease(value: CF);
    static kCFBooleanTrue: CF;
}
#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecItemCopyMatching(query: CF, result: *mut CF) -> i32;
    fn SecItemAdd(attributes: CF, result: *mut CF) -> i32;
    fn SecItemUpdate(query: CF, attributes: CF) -> i32;
    fn SecItemDelete(query: CF) -> i32;
    static kSecClass: CF;
    static kSecClassGenericPassword: CF;
    static kSecAttrService: CF;
    static kSecAttrAccount: CF;
    static kSecValueData: CF;
    static kSecReturnData: CF;
    static kSecMatchLimit: CF;
    static kSecMatchLimitOne: CF;
    #[cfg(target_os = "ios")]
    static kSecAttrAccessible: CF;
    #[cfg(target_os = "ios")]
    static kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly: CF;
}

struct Owned(CF);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}
fn string(value: &str) -> Result<Owned, String> {
    let value = unsafe {
        CFStringCreateWithBytes(ptr::null(), value.as_ptr(), value.len() as isize, UTF8, 0)
    };
    if value.is_null() {
        Err("无法创建钥匙串查询".into())
    } else {
        Ok(Owned(value))
    }
}
// No callbacks: all key/value owners outlive the synchronous Security call.
fn dictionary(entries: &[(CF, CF)]) -> Result<Owned, String> {
    let keys: Vec<_> = entries.iter().map(|e| e.0).collect();
    let values: Vec<_> = entries.iter().map(|e| e.1).collect();
    let value = unsafe {
        CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            entries.len() as isize,
            ptr::null(),
            ptr::null(),
        )
    };
    if value.is_null() {
        Err("无法创建钥匙串查询".into())
    } else {
        Ok(Owned(value))
    }
}
fn key(service: CF, account: CF) -> Vec<(CF, CF)> {
    unsafe {
        vec![
            (kSecClass, kSecClassGenericPassword),
            (kSecAttrService, service),
            (kSecAttrAccount, account),
        ]
    }
}
fn failure(status: i32) -> String {
    format!("系统钥匙串操作失败（{status}）")
}

pub fn read(service: &str, account: &str) -> Result<Option<String>, String> {
    let service = string(service)?;
    let account = string(account)?;
    let mut entries = key(service.0, account.0);
    unsafe {
        entries.extend([
            (kSecReturnData, kCFBooleanTrue),
            (kSecMatchLimit, kSecMatchLimitOne),
        ]);
    }
    let query = dictionary(&entries)?;
    let mut data = ptr::null();
    let status = unsafe { SecItemCopyMatching(query.0, &mut data) };
    let data = Owned(data);
    if status == NOT_FOUND {
        return Ok(None);
    }
    if status != 0 {
        return Err(failure(status));
    }
    if data.0.is_null() || unsafe { CFGetTypeID(data.0) != CFDataGetTypeID() } {
        return Err("系统钥匙串返回无效内容".into());
    }
    let length = unsafe { CFDataGetLength(data.0) };
    if !(0..=4_194_304).contains(&length) {
        return Err("配对凭据长度无效".into());
    }
    if length == 0 {
        return Ok(Some(String::new()));
    }
    let bytes = unsafe { CFDataGetBytePtr(data.0) };
    if bytes.is_null() {
        return Err("系统钥匙串返回无效内容".into());
    }
    String::from_utf8(unsafe { std::slice::from_raw_parts(bytes, length as usize) }.to_vec())
        .map(Some)
        .map_err(|_| "配对凭据编码无效".into())
}

pub fn write(service: &str, account: &str, value: &str) -> Result<(), String> {
    let service = string(service)?;
    let account = string(account)?;
    let data = Owned(unsafe { CFDataCreate(ptr::null(), value.as_ptr(), value.len() as isize) });
    if data.0.is_null() {
        return Err("无法创建钥匙串内容".into());
    }
    let mut entries = key(service.0, account.0);
    let query = dictionary(&entries)?;
    let changes = dictionary(&[(unsafe { kSecValueData }, data.0)])?;
    let status = unsafe { SecItemUpdate(query.0, changes.0) };
    if status == 0 {
        return Ok(());
    }
    if status != NOT_FOUND {
        return Err(failure(status));
    }
    entries.push((unsafe { kSecValueData }, data.0));
    #[cfg(target_os = "ios")]
    unsafe {
        entries.push((
            kSecAttrAccessible,
            kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ));
    }
    let attributes = dictionary(&entries)?;
    let status = unsafe { SecItemAdd(attributes.0, ptr::null_mut()) };
    if status == 0 {
        Ok(())
    } else {
        Err(failure(status))
    }
}

pub fn delete(service: &str, account: &str) -> Result<(), String> {
    let service = string(service)?;
    let account = string(account)?;
    let query = dictionary(&key(service.0, account.0))?;
    let status = unsafe { SecItemDelete(query.0) };
    if status == 0 || status == NOT_FOUND {
        Ok(())
    } else {
        Err(failure(status))
    }
}

/// 只添加缺失项，绝不覆盖；已存在时返回 false，由调用方重新读取核对。
#[cfg(target_os = "ios")]
pub fn add_only(service: &str, account: &str, value: &str) -> Result<bool, String> {
    let service = string(service)?;
    let account = string(account)?;
    let data = Owned(unsafe { CFDataCreate(ptr::null(), value.as_ptr(), value.len() as isize) });
    if data.0.is_null() {
        return Err("无法创建钥匙串内容".into());
    }
    let mut entries = key(service.0, account.0);
    unsafe {
        entries.push((kSecValueData, data.0));
        entries.push((
            kSecAttrAccessible,
            kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ));
    }
    let attributes = dictionary(&entries)?;
    match unsafe { SecItemAdd(attributes.0, ptr::null_mut()) } {
        0 => Ok(true),
        DUPLICATE => Ok(false),
        status => Err(failure(status)),
    }
}
