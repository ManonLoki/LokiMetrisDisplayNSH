//! 使用现有的 iOS 容器和私有钥匙串启动独立的 NSH。
use crate::features::connection::identity::{DEFAULT_NAME, apple_name_error};
use crate::native::{connection, identity};
use serde_json::{Value, json};
use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    ptr,
};

/// 无需 Swift 宿主,恢复相同的 iOS 身份与文件。
// 还原与旧版相同的 iOS 身份与文件，不需要 Swift 宿主
pub(super) fn configure(
    _root: &Path,
    // 设备标识（未使用）
    _device_id: &str,
    _name: &str,
) -> Result<connection::Config, String> {
    let bundle = bundle_id()?;
    // 设备身份存放在系统钥匙串
    let device_id = identity::device_identity(&format!("{bundle}.lan-identity-v1"))
        .map_err(|failure| failure.message().to_owned())?;
    // 此前的 FileManager applicationSupportDirectory 与 NSH iOS
    // 运行时都解析到这个沙盒目录。保持旧的所有者根目录。
    // 旧版的 applicationSupportDirectory 与 NSH 的 iOS 运行时都解析到同一个沙盒目录；沿用旧的 owner 根目录
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        // 否则报应用私有目录不可用
        .ok_or("应用私有目录不可用")?;
    let root = home.join("Library/Application Support");
    // 读取旧版保存的设备名称
    let saved = preference("mdns.deviceName").and_then(|value| value.as_str().map(str::to_owned));
    // 合法才采用，否则用默认名称
    let name = saved
        .filter(|value| apple_name_error(value).is_none())
        // 否则用默认名
        .unwrap_or_else(|| DEFAULT_NAME.into());
    // 组装配置：移动端身份由宿主提供，并为手表和小组件开启共享
    serde_json::from_value(json!({
        // 根目录、设备标识与名称
        "root": root, "deviceId": device_id, "name": name,
        // 宿主身份、平台与手表配套
        "hostIdentity": true, "platform": "ios", "watchCompanion": true,
        "appVersion": bundle_string("CFBundleShortVersionString").unwrap_or_else(|| "1.0".into()),
        // 小组件目录
        "widgetDir": crate::native::widgets::directory(),
    }))
    .map_err(|error| format!("iOS连接配置无效: {error}"))
}

// Core Foundation 对象引用
type Cf = *const c_void;
// UTF-8 编码常量
const UTF8: u32 = 0x0800_0100;

// Core Foundation 接口
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    // 主 Bundle
    fn CFBundleGetMainBundle() -> Cf;
    // Bundle 标识
    fn CFBundleGetIdentifier(bundle: Cf) -> Cf;
    // Info.plist 值
    fn CFBundleGetValueForInfoDictionaryKey(bundle: Cf, key: Cf) -> Cf;
    fn CFStringCreateWithBytes(
        allocator: Cf,
        bytes: *const u8,
        length: isize,
        encoding: u32,
        external: u8,
    ) -> Cf;
    fn CFStringGetCString(value: Cf, buffer: *mut u8, capacity: isize, encoding: u32) -> u8;
    fn CFPreferencesCopyAppValue(key: Cf, application: Cf) -> Cf;
    fn CFGetTypeID(value: Cf) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(value: Cf) -> u8;
    fn CFNumberGetTypeID() -> usize;
    fn CFNumberGetValue(value: Cf, kind: isize, result: *mut c_void) -> u8;
    fn CFRelease(value: Cf);
    static kCFPreferencesCurrentApplication: Cf;
}

// 拥有一个 Create/Copy 得到的引用，离开作用域时释放
struct Owned(Cf);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: 此包装器持有一个 Create/Copy 引用。
            unsafe { CFRelease(self.0) };
        }
    }
}

// 由文本创建 CFString
fn key(text: &str) -> Owned {
    // SAFETY: text 在调用期间是有效的 UTF-8 缓冲区。
    Owned(unsafe {
        // 按 UTF-8 字节创建字符串
        CFStringCreateWithBytes(ptr::null(), text.as_ptr(), text.len() as isize, UTF8, 0)
    })
}

// CFString 转 Rust 字符串；非字符串返回 None
fn string(value: Cf) -> Option<String> {
    // SAFETY: 调用方传入有效的 CoreFoundation 值或 NULL。
    if value.is_null() || unsafe { CFGetTypeID(value) != CFStringGetTypeID() } {
        return None;
    }
    let mut bytes = [0_u8; 4096];
    // SAFETY: 类型已检查,且可写缓冲区具有声明的容量。
    if unsafe { CFStringGetCString(value, bytes.as_mut_ptr(), bytes.len() as isize, UTF8) } == 0 {
        // 失败返回 None
        return None;
    }
    // 按 C 字符串读到第一个 NUL
    std::ffi::CStr::from_bytes_until_nul(&bytes)
        // 失败返回 None
        .ok()?
        .to_str()
        // 失败返回 None
        .ok()
        .map(str::to_owned)
}

fn bundle_id() -> Result<String, String> {
    // SAFETY: 主 bundle 是与进程同寿命、被借用的 CoreFoundation 对象。
    let bundle = unsafe { CFBundleGetMainBundle() };
    if bundle.is_null() {
        return Err("应用标识不可用".into());
    }
    // SAFETY: bundle 有效,且 CFBundleGetIdentifier 返回借用的字符串。
    string(unsafe { CFBundleGetIdentifier(bundle) })
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "应用标识不可用".into())
}

// 读取 Info.plist 中的字符串
fn bundle_string(name: &str) -> Option<String> {
    let key = key(name);
    // SAFETY: 主 bundle 由进程持有并被借用;key 持有它自己的 CFString。
    let bundle = unsafe { CFBundleGetMainBundle() };
    if bundle.is_null() || key.0.is_null() {
        return None;
    }
    // SAFETY: 在这次同步查找期间 CFBundle 和 key 都有效。
    // 读取信息字典里的值
    string(unsafe { CFBundleGetValueForInfoDictionaryKey(bundle, key.0) })
}

// 读取偏好值，支持字符串、布尔与整数
fn preference(name: &str) -> Option<Value> {
    let key = key(name);
    if key.0.is_null() {
        return None;
    }
    // SAFETY: key 有效;current-application 是不可变的 CF 常量。
    let value =
        // 读取偏好值并接管所有权
        Owned(unsafe { CFPreferencesCopyAppValue(key.0, kCFPreferencesCurrentApplication) });
    if value.0.is_null() {
        return None;
    }
    if let Some(text) = string(value.0) {
        return Some(Value::String(text));
    }
    // SAFETY: 复制出来的值在整个类型/值检查期间保持被持有。
    let kind = unsafe { CFGetTypeID(value.0) };
    // SAFETY: CF 类型标识函数只读取不可变的运行时元数据。
    if kind == unsafe { CFBooleanGetTypeID() } {
        // SAFETY: 已检查该值的 CFBoolean 类型。
        return Some(Value::Bool(unsafe { CFBooleanGetValue(value.0) } != 0));
    }
    // SAFETY: CF 类型标识函数只读取不可变的运行时元数据。
    // 整数（kCFNumberSInt64Type = 4）
    if kind == unsafe { CFNumberGetTypeID() } {
        let mut number = 0_i64;
        // SAFETY: 已检查 CFNumber 类型,kCFNumberSInt64Type=4,输出是 i64。
        if unsafe { CFNumberGetValue(value.0, 4, (&mut number as *mut i64).cast()) } != 0 {
            return Some(json!(number));
        }
    }
    None
}
