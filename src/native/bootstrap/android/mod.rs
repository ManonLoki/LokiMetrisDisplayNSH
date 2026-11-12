//! 使用现有的 Android 身份和加密保险库,引导独立的 NSH 应用。
// JNI 上下文
mod context;
// JNI 辅助函数
mod jni_support;
// Keystore 包裹密钥
mod keys;

// 引入名称与 ID 校验
use crate::features::connection::identity::{
    // 默认设备名、名称校验与设备标识校验
    android_default_name,
    // 名称校验
    android_name_error,
    // 设备标识校验
    valid_device_id,
};
// 引入 JNI 类型
use jni::{
    // JNI 环境
    JNIEnv,
    // JNI 对象与参数值
    objects::{JObject, JValue},
};
// 引入失败类型与结果别名
use jni_support::{Failure, Result};
// 引入 JSON
use serde_json::json;
// 引入路径
use std::path::{Path, PathBuf};

// 取应用 files 目录的规范路径
fn files_dir(env: &mut JNIEnv<'_>, context: &JObject<'_>) -> Result<PathBuf> {
    // 取得应用文件目录
    let directory = env
        // 调用 getFilesDir
        .call_method(context, "getFilesDir", "()Ljava/io/File;", &[])?
        // 取回返回值
        .l()?;
    // Android 应用的挂载命名空间可能把 /data/user/0 暴露为符号链接。
    // 在添加任何缓存文件路径之前,只解析这个由操作系统持有的根目录。
    // Android 的应用挂载命名空间可能把 /data/user/0 暴露成符号链接；只解析这个系统拥有的根，再拼接缓存文件路径
    let path = env
        // 调用 getCanonicalPath
        .call_method(directory, "getCanonicalPath", "()Ljava/lang/String;", &[])?
        // 取回返回值
        .l()?;
    // 读取路径文本并转换
    Ok(jni_support::text(env, path)?.into())
}

// 读取或生成设备身份与名称（沿用旧版的 SharedPreferences）
fn identity(env: &mut JNIEnv<'_>, context: &JObject<'_>) -> Result<(String, String)> {
    // 读取 mdns_identity 偏好
    let prefs = jni_support::preferences(env, context, "mdns_identity")?;
    // 已保存的设备标识
    let stored_id = jni_support::pref_string(env, &prefs, "device_id")?;
    // 已有合法身份沿用；非法的保留原记录并报错；没有则生成
    let id = match stored_id {
        // 标识合法则沿用
        Some(id) if valid_device_id(&id) => id,
        // 已有标识无效：报错并保留原记录
        Some(_) => return Err(jni_support::invalid("既有设备身份无效，已保留原记录。")),
        // 没有标识：新建 UUID
        None => uuid::Uuid::new_v4().to_string(),
    };
    // 已保存的设备名
    let stored_name = jni_support::pref_string(env, &prefs, "device_name")?;
    // 已有合法名称沿用，否则用设备型号作为默认名称
    let name = match stored_name.filter(|name| android_name_error(name).is_none()) {
        // 有则沿用
        Some(name) => name,
        // 没有名称
        None => {
            // 读取手机型号
            let model = env
                // 读取 Build.MODEL
                .get_static_field("android/os/Build", "MODEL", "Ljava/lang/String;")?
                // 取回返回值
                .l()?;
            // 由型号生成默认设备名
            android_default_name(&jni_support::text(env, model)?)
        }
    };
    // 写回规范化后的身份
    jni_support::save_strings(env, &prefs, &[("device_id", &id), ("device_name", &name)])?;
    // 返回标识与名称
    Ok((id, name))
}

// 读取旧版面板偏好
fn legacy_panel(env: &mut JNIEnv<'_>, context: &JObject<'_>) -> Result<serde_json::Value> {
    // 读取 mdns_identity 偏好
    let prefs = jni_support::preferences(env, context, "mdns_identity")?;
    // 读取全部已保存项
    let saved = env
        // 读取全部偏好
        .call_method(prefs, "getAll", "()Ljava/util/Map;", &[])?
        // 取回返回值
        .l()?;
    // 面板配置映射
    let mut panel = serde_json::Map::new();
    // 四个字段与旧版键名的对应
    for (key, stored) in [
        // 显示用量
        ("showUsage", "panel.show_usage"),
        // 显示 Hook
        ("showHooks", "panel.show_hooks"),
        // 用量详情
        ("usageDetail", "panel.usage_detail"),
        // Hook 布局
        ("hookLayout", "panel.hook_layout"),
        // 逐项处理
    ] {
        // 键转为文本
        let stored = jni_support::string(env, stored)?;
        // 取对应的值
        let value = env
            // 调用方法
            .call_method(
                // 目标对象
                &saved,
                // Java 类名或方法名参数
                "get",
                // JNI 方法签名
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                // 传给 Java 方法的参数
                &[JValue::Object(&stored)],
                // 参数结束
            )?
            // 取回返回值
            .l()?;
        // 值为空
        if value.is_null() {
            // 跳过
            continue;
        }
        // 布尔值
        if env.is_instance_of(&value, "java/lang/Boolean")? {
            // 按类型写入面板配置
            panel.insert(
                // 键
                key.into(),
                // 取布尔值
                json!(env.call_method(value, "booleanValue", "()Z", &[])?.z()?),
            );
        // 字符串值
        } else if env.is_instance_of(&value, "java/lang/String")? {
            // 其余按文本写入
            panel.insert(key.into(), json!(jni_support::text(env, value)?));
        }
    }
    // 没有任何值则为 null
    Ok(if panel.is_empty() {
        // 没有面板配置时为 null
        serde_json::Value::Null
    // 否则分支
    } else {
        // 返回面板配置
        panel.into()
    })
}

/// 使用旧的应用目录,并让机密完全留在原生代码内部。
// 使用旧的应用目录，秘密完全留在原生代码内
pub(super) fn configure(
    // 根目录
    _root: &Path,
    // 设备标识
    _device_id: &str,
    // 设备名
    _name: &str,
    // 返回连接配置
) -> std::result::Result<crate::native::connection::Config, String> {
    // 取得应用上下文并执行
    context::with_context(|env, context| -> Result<_> {
        // 应用文件目录
        let root = files_dir(env, context)?;
        // 设备标识与名称
        let (device_id, name) = identity(env, context)?;
        // 加载 Keystore 包裹的保险库密钥
        let mut key = keys::load(env, context, &root.join("loki-link-v1/vault"))?;
        // 配置保险库
        let result =
            // 配置保险库目录
            crate::native::credentials::configure_vault(root.join("loki-link-v1/vault"), key);
        // 用后清零密钥
        key.fill(0);
        // 失败转为错误
        result.map_err(Failure)?;
        // 组装配置：宿主提供身份，不保留离线快照
        serde_json::from_value(json!({
            // 根目录、设备标识、名称与宿主标识
            "root":root, "deviceId":device_id, "name":name, "hostIdentity":true,
            // 平台与不保留快照
            "platform":"android", "keepSnapshot":false,
            // 旧版面板配置
            "legacyPanel":legacy_panel(env, context)?,
            // 小组件共享目录：files/widget
            "widgetDir":Some(root.join("widget")),
        }))
        // 配置无效
        .map_err(|_| jni_support::invalid("Android 连接配置无效。"))
    })
    // 失败转为文本
    .map_err(|error: Failure| error.0)
}

/// 小组件共享目录：应用私有的 files/widget。小组件提供器经 JNI 读取同一路径，
/// 双方都必须使用这里给出的规范化形式，避免挂载命名空间的路径别名。
// 小组件共享目录，主应用与提供器共用
pub(super) fn widget_directory() -> Option<PathBuf> {
    // 取应用上下文并执行
    context::with_context(|env, context| {
        // 取规范化的文件目录并拼接小组件子目录
        Ok(files_dir(env, context)?.join("widget"))
    })
    // 失败则没有共享目录
    .ok()
}

/// 通知系统小组件有新摘要：调用 Java 侧静态刷新入口，在当前线程上重绘 RemoteViews。
// 通知 Android 系统小组件重绘
pub(super) fn widget_changed() {
    // 取应用上下文并执行；失败静默，不阻断连接事件
    let _: Result<()> = context::with_context(|env, context| {
        // 调用提供器的静态刷新方法
        env.call_static_method(
            // 提供器类
            "com/manonloki/lokimetrisdisplay/widget/LokiWidgetProvider",
            // 静态刷新方法
            "refreshAll",
            // 签名：接收应用上下文
            "(Landroid/content/Context;)V",
            // 传入应用对象
            &[JValue::Object(context)],
        )?;
        // 成功
        Ok(())
    });
}

// 获取多播锁，发现期间需要持有
fn multicast_lock() -> Result<Option<jni::objects::GlobalRef>> {
    // 取得应用上下文并执行
    context::with_context(|env, context| -> Result<_> {
        // Wi-Fi 服务
        let wifi = jni_support::system_service(env, context, "wifi")?;
        // 服务不存在
        if wifi.is_null() {
            // 不加锁
            return Ok(None);
        }
        // 锁的标签
        let tag = jni_support::string(env, "loki-lan-discovery")?;
        // 创建组播锁
        let lock = env
            // 调用方法
            .call_method(
                // Wi-Fi 管理器
                wifi,
                // Java 类名或方法名参数
                "createMulticastLock",
                // JNI 方法签名
                "(Ljava/lang/String;)Landroid/net/wifi/WifiManager$MulticastLock;",
                // 传给 Java 方法的参数
                &[JValue::Object(&tag)],
                // 参数结束
            )?
            // 取回返回值
            .l()?;
        // 关闭引用计数
        env.call_method(&lock, "setReferenceCounted", "(Z)V", &[JValue::Bool(0)])?;
        // 提升为全局引用
        let lock = env.new_global_ref(lock)?;
        // 获取锁
        env.call_method(lock.as_obj(), "acquire", "()V", &[])?;
        // 返回锁
        Ok(Some(lock))
    })
}

// 释放多播锁
fn release_multicast(lock: &jni::objects::GlobalRef) {
    // 取得应用上下文并执行
    let _: Result<()> = context::with_context(|env, _| {
        // 释放锁
        env.call_method(lock.as_obj(), "release", "()V", &[])?;
        // 成功返回
        Ok(())
    });
}

/// 界面可见时持有的多播锁，局域网发现期间需要；不可见或连接释放时归还。
#[derive(Default)]
pub(super) struct Multicast(std::sync::Mutex<Option<jni::objects::GlobalRef>>);

// 多播锁的方法
impl Multicast {
    // 随界面可见性获取或释放
    pub(super) fn set_visible(&self, visible: bool) {
        // 锁中毒时仍取出内部值
        let mut held = self.0.lock().unwrap_or_else(|p| p.into_inner());
        // 可见且尚未持有：获取
        if visible && held.is_none() {
            *held = multicast_lock().ok().flatten();
        // 不可见：释放
        } else if !visible && let Some(lock) = held.take() {
            release_multicast(&lock);
        }
    }
}

// 连接释放时归还多播锁
impl Drop for Multicast {
    // 销毁时
    fn drop(&mut self) {
        // 仍持有则释放
        if let Some(lock) = self.0.get_mut().unwrap_or_else(|p| p.into_inner()).take() {
            release_multicast(&lock);
        }
    }
}
