//! 使用现有的 iOS 容器和私有钥匙串启动独立的 NSH。
// 引入默认名称与 Apple 名称校验
use crate::features::connection::identity::{DEFAULT_NAME, apple_name_error};
// 引入连接、身份与生命周期
use crate::native::{connection, identity, lifecycle};
// 引入 JSON
use serde_json::{Value, json};
// 引入 FFI、路径与弱引用
use std::{
    // C 指针类型
    ffi::c_void,
    // 路径类型
    path::{Path, PathBuf},
    // 指针
    ptr,
    // 弱引用
    sync::Weak,
};

/// 无需 Swift 宿主,恢复相同的 iOS 身份与文件。
// 还原与旧版相同的 iOS 身份与文件，不需要 Swift 宿主
pub(super) fn configure(
    // 根目录（未使用）
    _root: &Path,
    // 设备标识（未使用）
    _device_id: &str,
    // 设备名（未使用）
    _name: &str,
    // 返回连接配置
) -> Result<connection::Config, String> {
    // 应用包标识
    let bundle = bundle_id()?;
    // 设备身份存放在系统钥匙串
    let device_id = identity::device_identity(&format!("{bundle}.lan-identity-v1"))
        // 失败转为文本
        .map_err(|failure| failure.message().to_owned())?;
    // 此前的 FileManager applicationSupportDirectory 与 NSH iOS
    // 运行时都解析到这个沙盒目录。保持旧的所有者根目录。
    // 旧版的 applicationSupportDirectory 与 NSH 的 iOS 运行时都解析到同一个沙盒目录；沿用旧的 owner 根目录
    let home = std::env::var_os("HOME")
        // 转为路径
        .map(PathBuf::from)
        // 须是绝对路径
        .filter(|path| path.is_absolute())
        // 否则报应用私有目录不可用
        .ok_or("应用私有目录不可用")?;
    // 应用支持目录
    let root = home.join("Library/Application Support");
    // 读取旧版保存的设备名称
    let saved = preference("mdns.deviceName").and_then(|value| value.as_str().map(str::to_owned));
    // 合法才采用，否则用默认名称
    let name = saved
        // 名称须合法
        .filter(|value| apple_name_error(value).is_none())
        // 否则用默认名
        .unwrap_or_else(|| DEFAULT_NAME.into());
    // 读取旧版面板偏好
    let legacy_panel = legacy_panel(&root);
    // 组装配置：移动端身份与生命周期由宿主提供，并为手表和小组件开启共享
    serde_json::from_value(json!({
        // 根目录、设备标识与名称
        "root": root, "deviceId": device_id, "name": name,
        // 宿主身份、宿主生命周期、平台与手表配套
        "hostIdentity": true, "hostLifecycle": true, "platform": "ios", "watchCompanion": true,
        // 应用版本
        "appVersion": bundle_string("CFBundleShortVersionString").unwrap_or_else(|| "1.0".into()),
        // 旧版面板配置与小组件目录
        "legacyPanel": legacy_panel, "widgetDir": crate::native::widgets::directory(),
    }))
    // 配置无效
    .map_err(|error| format!("iOS连接配置无效: {error}"))
}

/// 观察者只持有弱引用;关闭 VM 即关闭其连接。
// 观察器只持有弱引用；关闭 VM 就会关闭它的连接
pub(super) fn observe(handle: Weak<connection::Handle>) -> lifecycle::ActivityObserver {
    // 观察前后台变化
    lifecycle::observe(move |visible| {
        // 连接已释放
        let Some(handle) = handle.upgrade() else {
            // 返回
            return;
        };
        // 向 owner 推送前后台
        let command = json!({"op":"lifecycle","visible":visible,"permitted":true});
        // 把生命周期命令交给连接，失败忽略
        let _ = handle.host_command(&command.to_string());
    })
}

// 旧版面板偏好：优先读文件，其次读各个系统偏好键
fn legacy_panel(root: &Path) -> Option<Value> {
    // 文件不得超过 64 KiB，且必须是 JSON 对象
    let persisted = std::fs::read(root.join("panel-v1.json"))
        // 读取成功
        .ok()
        // 不超过 64 KiB
        .filter(|bytes| bytes.len() <= 64 * 1024)
        // 解析为 JSON
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        // 须是对象
        .filter(Value::is_object);
    // 没有保存的配置时读取系统偏好
    persisted.or_else(|| {
        // 偏好值映射
        let mut values = serde_json::Map::new();
        // 四个偏好字段与旧版键名的对应
        for (field, key) in [
            // 显示用量
            ("showUsage", "panel.showUsage"),
            // 显示 Hook
            ("showHooks", "panel.showHooks"),
            // 用量详情
            ("usageDetail", "panel.usageDetail"),
            // Hook 布局
            ("hookLayout", "panel.hookLayout"),
            // 逐项处理
        ] {
            // 读取偏好
            if let Some(value) = preference(key) {
                // 按字段名记录
                values.insert(field.into(), value);
            }
        }
        // 非空才返回对象
        (!values.is_empty()).then_some(Value::Object(values))
    })
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
    // 创建字符串
    fn CFStringCreateWithBytes(
        // 分配器
        allocator: Cf,
        // 字节
        bytes: *const u8,
        // 长度
        length: isize,
        // 编码
        encoding: u32,
        // 是否外部表示
        external: u8,
        // 返回字符串对象
    ) -> Cf;
    // 取 C 字符串
    fn CFStringGetCString(value: Cf, buffer: *mut u8, capacity: isize, encoding: u32) -> u8;
    // 读取偏好
    fn CFPreferencesCopyAppValue(key: Cf, application: Cf) -> Cf;
    // 类型 ID
    fn CFGetTypeID(value: Cf) -> usize;
    // 字符串类型编号
    fn CFStringGetTypeID() -> usize;
    // 布尔类型编号
    fn CFBooleanGetTypeID() -> usize;
    // 读取布尔值
    fn CFBooleanGetValue(value: Cf) -> u8;
    // 数字类型编号
    fn CFNumberGetTypeID() -> usize;
    // 读取数字值
    fn CFNumberGetValue(value: Cf, kind: isize, result: *mut c_void) -> u8;
    // 释放对象
    fn CFRelease(value: Cf);
    // 当前应用的偏好域
    static kCFPreferencesCurrentApplication: Cf;
}

// 拥有一个 Create/Copy 得到的引用，离开作用域时释放
struct Owned(Cf);
// 持有对象的析构
impl Drop for Owned {
    // 销毁时
    fn drop(&mut self) {
        // 非空才释放
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
        // 返回 None
        return None;
    }
    // 读取缓冲区
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
        // 转为字符串
        .to_str()
        // 失败返回 None
        .ok()
        // 复制为自有字符串
        .map(str::to_owned)
}

// 取应用包标识
fn bundle_id() -> Result<String, String> {
    // SAFETY: 主 bundle 是与进程同寿命、被借用的 CoreFoundation 对象。
    let bundle = unsafe { CFBundleGetMainBundle() };
    // 包对象为空
    if bundle.is_null() {
        // 应用标识不可用
        return Err("应用标识不可用".into());
    }
    // SAFETY: bundle 有效,且 CFBundleGetIdentifier 返回借用的字符串。
    string(unsafe { CFBundleGetIdentifier(bundle) })
        // 空值不采用
        .filter(|value| !value.is_empty())
        // 缺失则报错
        .ok_or_else(|| "应用标识不可用".into())
}

// 读取 Info.plist 中的字符串
fn bundle_string(name: &str) -> Option<String> {
    // 键转为系统字符串
    let key = key(name);
    // SAFETY: 主 bundle 由进程持有并被借用;key 持有它自己的 CFString。
    let bundle = unsafe { CFBundleGetMainBundle() };
    // 包或键为空
    if bundle.is_null() || key.0.is_null() {
        // 返回 None
        return None;
    }
    // SAFETY: 在这次同步查找期间 CFBundle 和 key 都有效。
    // 读取信息字典里的值
    string(unsafe { CFBundleGetValueForInfoDictionaryKey(bundle, key.0) })
}

// 读取偏好值，支持字符串、布尔与整数
fn preference(name: &str) -> Option<Value> {
    // 键转为系统字符串
    let key = key(name);
    // 创建失败
    if key.0.is_null() {
        // 返回 None
        return None;
    }
    // SAFETY: key 有效;current-application 是不可变的 CF 常量。
    let value =
        // 读取偏好值并接管所有权
        Owned(unsafe { CFPreferencesCopyAppValue(key.0, kCFPreferencesCurrentApplication) });
    // 空指针
    if value.0.is_null() {
        // 返回 None
        return None;
    }
    // 字符串
    if let Some(text) = string(value.0) {
        // 返回字符串
        return Some(Value::String(text));
    }
    // SAFETY: 复制出来的值在整个类型/值检查期间保持被持有。
    let kind = unsafe { CFGetTypeID(value.0) };
    // SAFETY: CF 类型标识函数只读取不可变的运行时元数据。
    // 布尔
    if kind == unsafe { CFBooleanGetTypeID() } {
        // SAFETY: 已检查该值的 CFBoolean 类型。
        return Some(Value::Bool(unsafe { CFBooleanGetValue(value.0) } != 0));
    }
    // SAFETY: CF 类型标识函数只读取不可变的运行时元数据。
    // 整数（kCFNumberSInt64Type = 4）
    if kind == unsafe { CFNumberGetTypeID() } {
        // 数值缓冲
        let mut number = 0_i64;
        // SAFETY: 已检查 CFNumber 类型,kCFNumberSInt64Type=4,输出是 i64。
        if unsafe { CFNumberGetValue(value.0, 4, (&mut number as *mut i64).cast()) } != 0 {
            // 返回数字
            return Some(json!(number));
        }
    }
    // 其他类型不支持
    None
}
