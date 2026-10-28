//! Apple Keychain 的所有权与返回码只在此处处理，调用方不接触原生指针。
// 引入 C 指针类型与空指针
use std::{ffi::c_void, ptr};

// Core Foundation 对象引用
type CF = *const c_void;
// UTF-8 编码常量
const UTF8: u32 = 0x0800_0100;
// errSecItemNotFound
const NOT_FOUND: i32 = -25300;
#[cfg(target_os = "ios")]
// errSecDuplicateItem
const DUPLICATE: i32 = -25299;

// Core Foundation 接口
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    // 由字节创建字符串
    fn CFStringCreateWithBytes(
        // 分配器
        allocator: CF,
        // 字节
        bytes: *const u8,
        // 长度
        length: isize,
        // 编码
        encoding: u32,
        // 是否外部表示
        external: u8,
        // 返回字符串对象
    ) -> CF;
    // 创建数据对象
    fn CFDataCreate(allocator: CF, bytes: *const u8, length: isize) -> CF;
    // 数据长度
    fn CFDataGetLength(data: CF) -> isize;
    // 数据指针
    fn CFDataGetBytePtr(data: CF) -> *const u8;
    // 对象类型 ID
    fn CFGetTypeID(value: CF) -> usize;
    // 数据类型 ID
    fn CFDataGetTypeID() -> usize;
    // 创建字典
    fn CFDictionaryCreate(
        // 分配器
        allocator: CF,
        // 键数组
        keys: *const CF,
        // 值数组
        values: *const CF,
        // 条目数量
        count: isize,
        // 键回调
        key_callbacks: CF,
        // 值回调
        value_callbacks: CF,
        // 返回字典对象
    ) -> CF;
    // 释放对象
    fn CFRelease(value: CF);
    // 布尔真值
    static kCFBooleanTrue: CF;
}
// Keychain 接口
#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    // 查询条目
    fn SecItemCopyMatching(query: CF, result: *mut CF) -> i32;
    // 添加条目
    fn SecItemAdd(attributes: CF, result: *mut CF) -> i32;
    // 更新条目
    fn SecItemUpdate(query: CF, attributes: CF) -> i32;
    // 删除条目
    fn SecItemDelete(query: CF) -> i32;
    // 查询键：条目类别、服务、账户、数据、返回数据、匹配上限
    static kSecClass: CF;
    // 通用密码类别
    static kSecClassGenericPassword: CF;
    // 服务属性
    static kSecAttrService: CF;
    // 账户属性
    static kSecAttrAccount: CF;
    // 数据值
    static kSecValueData: CF;
    // 是否返回数据
    static kSecReturnData: CF;
    // 匹配数量上限
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
// 持有对象的析构
impl Drop for Owned {
    // 销毁时
    fn drop(&mut self) {
        // 非空才释放
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
    // 创建失败
    if value.is_null() {
        // 报无法创建钥匙串查询
        Err("无法创建钥匙串查询".into())
    // 否则分支
    } else {
        // 返回持有的字符串
        Ok(Owned(value))
    }
}
// 没有回调:所有键/值的持有者都比这次同步的 Security 调用活得久。
// 创建不带回调的字典：所有键值的拥有者都比这次同步的 Security 调用活得久
fn dictionary(entries: &[(CF, CF)]) -> Result<Owned, String> {
    // 键列表
    let keys: Vec<_> = entries.iter().map(|e| e.0).collect();
    // 值列表
    let values: Vec<_> = entries.iter().map(|e| e.1).collect();
    // 创建字典
    let value = unsafe {
        // 调用系统接口
        CFDictionaryCreate(
            // 键回调为空
            ptr::null(),
            // 键数组
            keys.as_ptr(),
            // 值数组
            values.as_ptr(),
            // 条目数量
            entries.len() as isize,
            // 键回调为空
            ptr::null(),
            // 值回调为空
            ptr::null(),
        )
    };
    // 创建失败
    if value.is_null() {
        // 报错
        Err("无法创建钥匙串查询".into())
    // 否则
    } else {
        // 返回持有的字典
        Ok(Owned(value))
    }
}
// 通用密码条目的基本键：类别、服务和账户
fn key(service: CF, account: CF) -> Vec<(CF, CF)> {
    // 调用系统接口
    unsafe {
        // 组装条目
        vec![
            // 类别：通用密码
            (kSecClass, kSecClassGenericPassword),
            // 服务
            (kSecAttrService, service),
            // 账户
            (kSecAttrAccount, account),
        ]
    }
}
// 把返回码转为文案
fn failure(status: i32) -> String {
    // 失败信息带状态码
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
    // 调用系统接口
    unsafe {
        // 加入查询条件
        entries.extend([
            // 返回数据
            (kSecReturnData, kCFBooleanTrue),
            // 只匹配一条
            (kSecMatchLimit, kSecMatchLimitOne),
        ]);
    }
    // 构造查询字典
    let query = dictionary(&entries)?;
    // 接收结果的指针
    let mut data = ptr::null();
    // 查询钥匙串
    let status = unsafe { SecItemCopyMatching(query.0, &mut data) };
    // 接管结果的所有权
    let data = Owned(data);
    // 没有找到条目
    if status == NOT_FOUND {
        // 返回 None
        return Ok(None);
    }
    // 其他错误
    if status != 0 {
        // 返回失败信息
        return Err(failure(status));
    }
    // 返回内容必须是数据对象
    if data.0.is_null() || unsafe { CFGetTypeID(data.0) != CFDataGetTypeID() } {
        // 返回无效内容错误
        return Err("系统钥匙串返回无效内容".into());
    }
    // 长度限制在 4 MiB 内
    let length = unsafe { CFDataGetLength(data.0) };
    // 长度须在 0 到 4 MiB
    if !(0..=4_194_304).contains(&length) {
        // 否则报长度无效
        return Err("配对凭据长度无效".into());
    }
    // 长度为 0
    if length == 0 {
        // 返回空字符串
        return Ok(Some(String::new()));
    }
    // 取数据指针
    let bytes = unsafe { CFDataGetBytePtr(data.0) };
    // 数据指针为空
    if bytes.is_null() {
        // 返回无效内容错误
        return Err("系统钥匙串返回无效内容".into());
    }
    // 必须是合法 UTF-8
    String::from_utf8(unsafe { std::slice::from_raw_parts(bytes, length as usize) }.to_vec())
        // 包装为 Some
        .map(Some)
        // 编码损坏则报错
        .map_err(|_| "配对凭据编码无效".into())
}

// 写入凭据：先尝试更新，不存在再添加
pub fn write(service: &str, account: &str, value: &str) -> Result<(), String> {
    // 服务名转为系统字符串
    let service = string(service)?;
    // 账户名转为系统字符串
    let account = string(account)?;
    // 创建数据对象
    let data = Owned(unsafe { CFDataCreate(ptr::null(), value.as_ptr(), value.len() as isize) });
    // 创建失败
    if data.0.is_null() {
        // 报无法创建钥匙串内容
        return Err("无法创建钥匙串内容".into());
    }
    // 条目的基本键
    let mut entries = key(service.0, account.0);
    // 构造查询
    let query = dictionary(&entries)?;
    // 构造要更新的字段
    let changes = dictionary(&[(unsafe { kSecValueData }, data.0)])?;
    // 执行更新
    let status = unsafe { SecItemUpdate(query.0, changes.0) };
    // 更新成功
    if status == 0 {
        // 返回成功
        return Ok(());
    }
    // 其他错误直接失败
    if status != NOT_FOUND {
        // 返回失败信息
        return Err(failure(status));
    }
    // 不存在则添加
    entries.push((unsafe { kSecValueData }, data.0));
    // iOS：仅首次解锁后可访问，且不随设备迁移
    #[cfg(target_os = "ios")]
    unsafe {
        // 加入数据与可访问性
        entries.push((
            // 可访问性键
            kSecAttrAccessible,
            // 首次解锁后且仅限本设备
            kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ));
    }
    // 构造属性字典
    let attributes = dictionary(&entries)?;
    // 执行添加
    let status = unsafe { SecItemAdd(attributes.0, ptr::null_mut()) };
    // 成功
    if status == 0 {
        // 返回 Ok
        Ok(())
    // 否则
    } else {
        // 返回失败信息
        Err(failure(status))
    }
}

// 删除凭据；不存在也视为成功
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    // 服务名转为系统字符串
    let service = string(service)?;
    // 账户名转为系统字符串
    let account = string(account)?;
    // 构造查询字典
    let query = dictionary(&key(service.0, account.0))?;
    // 执行删除
    let status = unsafe { SecItemDelete(query.0) };
    // 成功或本就不存在
    if status == 0 || status == NOT_FOUND {
        // 返回成功
        Ok(())
    // 否则分支
    } else {
        // 返回失败信息
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
    // 创建数据对象
    let data = Owned(unsafe { CFDataCreate(ptr::null(), value.as_ptr(), value.len() as isize) });
    // 创建失败
    if data.0.is_null() {
        // 报无法创建钥匙串内容
        return Err("无法创建钥匙串内容".into());
    }
    // 条目的基本键
    let mut entries = key(service.0, account.0);
    // 调用系统接口
    unsafe {
        // 加入数据
        entries.push((kSecValueData, data.0));
        // 加入可访问性
        entries.push((
            // 键：可访问性
            kSecAttrAccessible,
            // 值：首次解锁后且仅限本设备
            kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ));
    }
    // 构造属性字典
    let attributes = dictionary(&entries)?;
    // 0 表示新增成功，重复表示已存在
    match unsafe { SecItemAdd(attributes.0, ptr::null_mut()) } {
        // 状态为 0：成功
        0 => Ok(true),
        // 已存在则返回 false
        DUPLICATE => Ok(false),
        // 其他状态报错
        status => Err(failure(status)),
    }
}
