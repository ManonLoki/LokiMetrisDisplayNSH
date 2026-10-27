//! Small JNI conversions shared by Android's bootstrap and Keystore adapter.
use jni::{
    JNIEnv,
    objects::{JObject, JString, JValue},
};

#[derive(Debug)]
pub(super) struct Failure(pub(super) String);

impl From<jni::errors::Error> for Failure {
    fn from(_: jni::errors::Error) -> Self {
        Self("Android 系统存储访问失败，请重新打开应用。".into())
    }
}

pub(super) type Result<T> = std::result::Result<T, Failure>;

pub(super) fn invalid(message: &str) -> Failure {
    Failure(message.into())
}

pub(super) fn string<'local>(env: &mut JNIEnv<'local>, value: &str) -> Result<JObject<'local>> {
    Ok(env.new_string(value)?.into())
}

pub(super) fn text(env: &mut JNIEnv<'_>, value: JObject<'_>) -> Result<String> {
    if value.is_null() {
        return Err(invalid("Android 系统返回了空字符串。"));
    }
    Ok(env.get_string(&JString::from(value))?.into())
}

pub(super) fn preferences<'local>(
    env: &mut JNIEnv<'local>,
    context: &JObject<'_>,
    name: &str,
) -> Result<JObject<'local>> {
    let name = string(env, name)?;
    Ok(env
        .call_method(
            context,
            "getSharedPreferences",
            "(Ljava/lang/String;I)Landroid/content/SharedPreferences;",
            &[JValue::Object(&name), JValue::Int(0)],
        )?
        .l()?)
}

pub(super) fn pref_string(
    env: &mut JNIEnv<'_>,
    prefs: &JObject<'_>,
    key: &str,
) -> Result<Option<String>> {
    let key = string(env, key)?;
    let value = env
        .call_method(
            prefs,
            "getString",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&key), JValue::Object(&JObject::null())],
        )?
        .l()?;
    if value.is_null() {
        Ok(None)
    } else {
        text(env, value).map(Some)
    }
}

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
        let key = string(env, key)?;
        let value = string(env, value)?;
        env.call_method(
            &editor,
            "putString",
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
            &[JValue::Object(&key), JValue::Object(&value)],
        )?;
    }
    if !env.call_method(editor, "commit", "()Z", &[])?.z()? {
        return Err(invalid("Android 系统存储写入失败，请重新打开应用。"));
    }
    Ok(())
}

pub(super) fn system_service<'local>(
    env: &mut JNIEnv<'local>,
    context: &JObject<'_>,
    name: &str,
) -> Result<JObject<'local>> {
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
