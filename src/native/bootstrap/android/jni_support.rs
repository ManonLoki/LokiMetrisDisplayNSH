//! Android 引导程序与 Keystore 适配器共用的小型 JNI 转换。
// 引入 JNI 类型
use jni::{
    // JNI 环境
    JNIEnv,
    // 对象、字符串与参数值类型
    objects::{JObject, JString, JValue},
};

#[derive(Debug)]
// 面向用户的失败文案
pub(super) struct Failure(pub(super) String);

// JNI 错误统一转为通用文案，不泄露细节
impl From<jni::errors::Error> for Failure {
    // 丢弃具体的 JNI 错误
    fn from(_: jni::errors::Error) -> Self {
        // 统一的用户文案
        Self("Android 系统存储访问失败，请重新打开应用。".into())
    }
}

// 结果别名
pub(super) type Result<T> = std::result::Result<T, Failure>;

// 构造失败
pub(super) fn invalid(message: &str) -> Failure {
    // 包装为失败
    Failure(message.into())
}

// Rust 字符串转 Java 字符串
pub(super) fn string<'local>(env: &mut JNIEnv<'local>, value: &str) -> Result<JObject<'local>> {
    // 创建 Java 字符串并转为通用对象
    Ok(env.new_string(value)?.into())
}

// Java 字符串转 Rust 字符串，空则报错
pub(super) fn text(env: &mut JNIEnv<'_>, value: JObject<'_>) -> Result<String> {
    // 空对象视为失败
    if value.is_null() {
        // 系统返回空字符串
        return Err(invalid("Android 系统返回了空字符串。"));
    }
    // 取出 Java 字符串内容
    Ok(env.get_string(&JString::from(value))?.into())
}

// 取得名为 name 的 SharedPreferences
pub(super) fn preferences<'local>(
    // JNI 环境
    env: &mut JNIEnv<'local>,
    // Android 上下文
    context: &JObject<'_>,
    // 偏好文件名
    name: &str,
    // 返回 Java 对象
) -> Result<JObject<'local>> {
    // 文件名转为 Java 字符串
    let name = string(env, name)?;
    // 调用 Context.getSharedPreferences(name, MODE_PRIVATE)
    Ok(env
        // 调用方法
        .call_method(
            // 调用对象
            context,
            // 方法名
            "getSharedPreferences",
            // 方法签名
            "(Ljava/lang/String;I)Landroid/content/SharedPreferences;",
            // 参数：名称与模式 0（私有）
            &[JValue::Object(&name), JValue::Int(0)],
            // 参数结束
        )?
        // 取返回的对象
        .l()?)
}

// 读取字符串偏好，不存在返回 None
pub(super) fn pref_string(
    // JNI 环境
    env: &mut JNIEnv<'_>,
    // 偏好对象
    prefs: &JObject<'_>,
    // 键
    key: &str,
    // 返回可选字符串
) -> Result<Option<String>> {
    // 键转为 Java 字符串
    let key = string(env, key)?;
    // 调用 getString(key, null)
    let value = env
        // 调用 getString
        .call_method(
            // 偏好对象
            prefs,
            // 方法名
            "getString",
            // 方法签名
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            // 默认值为 null
            &[JValue::Object(&key), JValue::Object(&JObject::null())],
            // 参数结束
        )?
        // 取对象结果
        .l()?;
    // 不存在
    if value.is_null() {
        // 没有值
        Ok(None)
    // 否则分支
    } else {
        // 存在则转为 Rust 字符串
        text(env, value).map(Some)
    }
}

// 一次提交多个字符串偏好；commit 失败则报错
pub(super) fn save_strings(
    // JNI 环境
    env: &mut JNIEnv<'_>,
    // 偏好对象
    prefs: &JObject<'_>,
    // 要保存的键值对
    values: &[(&str, &str)],
    // 返回结果
) -> Result<()> {
    // 取得编辑器
    let editor = env
        // 调用 edit
        .call_method(
            // 偏好对象
            prefs,
            // 方法名
            "edit",
            // 方法签名
            "()Landroid/content/SharedPreferences$Editor;",
            // 无参数
            &[],
            // 参数结束
        )?
        // 取回返回值
        .l()?;
    // 逐项写入
    for (key, value) in values {
        // 键转为 Java 字符串
        let key = string(env, key)?;
        // 值转为 Java 字符串
        let value = string(env, value)?;
        // 调用写入方法
        env.call_method(
            // 编辑器
            &editor,
            // 方法名
            "putString",
            // 方法签名
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
            // 键与值
            &[JValue::Object(&key), JValue::Object(&value)],
            // 调用结束，传播 JNI 错误
        )?;
    }
    // 同步提交，失败则报错
    if !env.call_method(editor, "commit", "()Z", &[])?.z()? {
        // 写入失败
        return Err(invalid("Android 系统存储写入失败，请重新打开应用。"));
    }
    // 完成
    Ok(())
}

// 取得系统服务
pub(super) fn system_service<'local>(
    // JNI 环境
    env: &mut JNIEnv<'local>,
    // 应用上下文
    context: &JObject<'_>,
    // 服务名
    name: &str,
    // 返回 Java 对象
) -> Result<JObject<'local>> {
    // 名称转为 Java 字符串
    let name = string(env, name)?;
    // 获取系统服务
    Ok(env
        // 调用方法
        .call_method(
            // 上下文
            context,
            // 方法名
            "getSystemService",
            // 方法签名
            "(Ljava/lang/String;)Ljava/lang/Object;",
            // 参数：名称对象
            &[JValue::Object(&name)],
            // 出错即返回
        )?
        // 取回返回值
        .l()?)
}
