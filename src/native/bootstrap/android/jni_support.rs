//! Android 引导程序与 Keystore 适配器共用的小型 JNI 转换。
use jni::{
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
        Self("Android 系统存储访问失败，请重新打开应用。".into())
    }
}

pub(super) type Result<T> = std::result::Result<T, Failure>;

pub(super) fn invalid(message: &str) -> Failure {
    Failure(message.into())
}

// Rust 字符串转 Java 字符串
pub(super) fn string<'local>(env: &mut JNIEnv<'local>, value: &str) -> Result<JObject<'local>> {
    // 创建 Java 字符串并转为通用对象
    Ok(env.new_string(value)?.into())
}

// Java 字符串转 Rust 字符串，空则报错
pub(super) fn text(env: &mut JNIEnv<'_>, value: JObject<'_>) -> Result<String> {
    if value.is_null() {
        return Err(invalid("Android 系统返回了空字符串。"));
    }
    // 取出 Java 字符串内容
    Ok(env.get_string(&JString::from(value))?.into())
}

// 取得名为 name 的 SharedPreferences
pub(super) fn preferences<'local>(
    env: &mut JNIEnv<'local>,
    // Android 上下文
    context: &JObject<'_>,
    name: &str,
    // 返回 Java 对象
) -> Result<JObject<'local>> {
    // 文件名转为 Java 字符串
    let name = string(env, name)?;
    // 调用 Context.getSharedPreferences(name, MODE_PRIVATE)
    Ok(env
        .call_method(
            context,
            "getSharedPreferences",
            "(Ljava/lang/String;I)Landroid/content/SharedPreferences;",
            // 参数：名称与模式 0（私有）
            &[JValue::Object(&name), JValue::Int(0)],
        )?
        .l()?)
}

// 读取字符串偏好，不存在返回 None
pub(super) fn pref_string(
    env: &mut JNIEnv<'_>,
    prefs: &JObject<'_>,
    key: &str,
) -> Result<Option<String>> {
    // 键转为 Java 字符串
    let key = string(env, key)?;
    // 调用 getString(key, null)
    let value = env
        // 调用 getString
        .call_method(
            prefs,
            "getString",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            // 默认值为 null
            &[JValue::Object(&key), JValue::Object(&JObject::null())],
        )?
        .l()?;
    if value.is_null() {
        Ok(None)
    // 否则分支
    } else {
        // 存在则转为 Rust 字符串
        text(env, value).map(Some)
    }
}

// 一次提交多个字符串偏好；commit 失败则报错
pub(super) fn save_strings(
    env: &mut JNIEnv<'_>,
    prefs: &JObject<'_>,
    values: &[(&str, &str)],
) -> Result<()> {
    let editor = env
        .call_method(
            prefs,
            "edit",
            "()Landroid/content/SharedPreferences$Editor;",
            &[],
        )?
        .l()?;
    for (key, value) in values {
        // 键转为 Java 字符串
        let key = string(env, key)?;
        // 值转为 Java 字符串
        let value = string(env, value)?;
        env.call_method(
            &editor,
            "putString",
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
            &[JValue::Object(&key), JValue::Object(&value)],
            // 调用结束，传播 JNI 错误
        )?;
    }
    // 同步提交，失败则报错
    if !env.call_method(editor, "commit", "()Z", &[])?.z()? {
        return Err(invalid("Android 系统存储写入失败，请重新打开应用。"));
    }
    Ok(())
}

pub(super) fn system_service<'local>(
    env: &mut JNIEnv<'local>,
    context: &JObject<'_>,
    name: &str,
    // 返回 Java 对象
) -> Result<JObject<'local>> {
    // 名称转为 Java 字符串
    let name = string(env, name)?;
    Ok(env
        .call_method(
            context,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&name)],
        )?
        .l()?)
}
