//! Apple Keychain 的所有权与返回码只在此处处理，调用方不接触原生指针。
use std::{ffi::c_void, ptr};

// Core Foundation 对象引用
type CF = *const c_void;
// UTF-8 编码常量
const UTF8: u32 = 0x0800_0100;
// errSecItemNotFound:条目不存在
const NOT_FOUND: i32 = -25300;
#[cfg(target_os = "ios")]
// errSecDuplicateItem:条目已存在
const DUPLICATE: i32 = -25299;

// Core Foundation 接口
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
// Keychain 接口
#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecItemCopyMatching(query: CF, result: *mut CF) -> i32;
    fn SecItemAdd(attributes: CF, result: *mut CF) -> i32;
    fn SecItemUpdate(query: CF, attributes: CF) -> i32;
    fn SecItemDelete(query: CF) -> i32;
    // 查询键：条目类别、服务、账户、数据、返回数据、匹配上限
    static kSecClass: CF;
    static kSecClassGenericPassword: CF;
    static kSecAttrService: CF;
    static kSecAttrAccount: CF;
    static kSecValueData: CF;
    static kSecReturnData: CF;
    static kSecMatchLimit: CF;
    // 只匹配一条
    static kSecMatchLimitOne: CF;
    #[cfg(target_os = "ios")]
    // 仅 iOS：首次解锁后可访问且不迁移到其他设备
    static kSecAttrAccessible: CF;
    #[cfg(target_os = "ios")]
    static kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly: CF;
}

// 拥有一个 CF 对象，离开作用域时释放
struct Owned(CF);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // 释放 Core Foundation 对象
            unsafe { CFRelease(self.0) };
        }
    }
}
// 创建 CF 字符串
fn string(value: &str) -> Result<Owned, String> {
    // 创建 Core Foundation 字符串
    let value = unsafe {
        // 按 UTF-8 字节创建
        CFStringCreateWithBytes(ptr::null(), value.as_ptr(), value.len() as isize, UTF8, 0)
    };
    if value.is_null() {
        // 报无法创建钥匙串查询
        Err("无法创建钥匙串查询".into())
    // 否则分支
    } else {
        Ok(Owned(value))
    }
}
// 没有回调:所有键/值的持有者都比这次同步的 Security 调用活得久。
// 创建不带回调的字典：所有键值的拥有者都比这次同步的 Security 调用活得久
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
    // 否则
    } else {
        Ok(Owned(value))
    }
}
// 通用密码条目的基本键：类别、服务和账户
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

// 读取凭据；不存在返回 None
pub fn read(service: &str, account: &str) -> Result<Option<String>, String> {
    // 服务名转为系统字符串
    let service = string(service)?;
    // 账户名转为系统字符串
    let account = string(account)?;
    // 在基本键上要求返回数据且只匹配一项
    let mut entries = key(service.0, account.0);
    unsafe {
        entries.extend([
            (kSecReturnData, kCFBooleanTrue),
            // 只匹配一条
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
    // 返回内容必须是数据对象
    if data.0.is_null() || unsafe { CFGetTypeID(data.0) != CFDataGetTypeID() } {
        return Err("系统钥匙串返回无效内容".into());
    }
    // 长度限制在 4 MiB 内
    let length = unsafe { CFDataGetLength(data.0) };
    // 长度须在 0 到 4 MiB
    if !(0..=4_194_304).contains(&length) {
        // 否则报长度无效
        return Err("配对凭据长度无效".into());
    }
    if length == 0 {
        return Ok(Some(String::new()));
    }
    let bytes = unsafe { CFDataGetBytePtr(data.0) };
    if bytes.is_null() {
        return Err("系统钥匙串返回无效内容".into());
    }
    // 必须是合法 UTF-8
    String::from_utf8(unsafe { std::slice::from_raw_parts(bytes, length as usize) }.to_vec())
        .map(Some)
        .map_err(|_| "配对凭据编码无效".into())
}

// 写入凭据：先尝试更新，不存在再添加
pub fn write(service: &str, account: &str, value: &str) -> Result<(), String> {
    // 服务名转为系统字符串
    let service = string(service)?;
    // 账户名转为系统字符串
    let account = string(account)?;
    let data = Owned(unsafe { CFDataCreate(ptr::null(), value.as_ptr(), value.len() as isize) });
    if data.0.is_null() {
        // 报无法创建钥匙串内容
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
    // iOS：仅首次解锁后可访问，且不随设备迁移
    #[cfg(target_os = "ios")]
    unsafe {
        // 加入数据与可访问性
        entries.push((
            kSecAttrAccessible,
            // 首次解锁后且仅限本设备
            kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ));
    }
    let attributes = dictionary(&entries)?;
    let status = unsafe { SecItemAdd(attributes.0, ptr::null_mut()) };
    if status == 0 {
        Ok(())
    // 否则
    } else {
        Err(failure(status))
    }
}

// 删除凭据；不存在也视为成功
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    // 服务名转为系统字符串
    let service = string(service)?;
    // 账户名转为系统字符串
    let account = string(account)?;
    let query = dictionary(&key(service.0, account.0))?;
    let status = unsafe { SecItemDelete(query.0) };
    if status == 0 || status == NOT_FOUND {
        Ok(())
    // 否则分支
    } else {
        Err(failure(status))
    }
}

/// 只添加缺失项，绝不覆盖；已存在时返回 false，由调用方重新读取核对。
#[cfg(target_os = "ios")]
// 只添加缺失项，绝不覆盖；已存在返回 false，由调用方重新读取核对
pub fn add_only(service: &str, account: &str, value: &str) -> Result<bool, String> {
    // 服务名转为系统字符串
    let service = string(service)?;
    // 账户名转为系统字符串
    let account = string(account)?;
    let data = Owned(unsafe { CFDataCreate(ptr::null(), value.as_ptr(), value.len() as isize) });
    if data.0.is_null() {
        // 报无法创建钥匙串内容
        return Err("无法创建钥匙串内容".into());
    }
    let mut entries = key(service.0, account.0);
    unsafe {
        entries.push((kSecValueData, data.0));
        entries.push((
            kSecAttrAccessible,
            // 值：首次解锁后且仅限本设备
            kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ));
    }
    let attributes = dictionary(&entries)?;
    // 0 表示新增成功，重复表示已存在
    match unsafe { SecItemAdd(attributes.0, ptr::null_mut()) } {
        0 => Ok(true),
        // 已存在则返回 false
        DUPLICATE => Ok(false),
        status => Err(failure(status)),
    }
}
