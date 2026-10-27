//! 应用进程拥有 JNI 上下文；不保存 Activity，也不读取 SDK 的全局状态。
use super::jni_support::{Failure, Result, invalid};
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JClass, JObject},
};
use std::sync::OnceLock;

struct Context {
    vm: JavaVM,
    application: GlobalRef,
}

static CONTEXT: OnceLock<Context> = OnceLock::new();

fn install(env: &mut JNIEnv<'_>, application: &JObject<'_>) -> Result<()> {
    let application = env
        .call_method(
            application,
            "getApplicationContext",
            "()Landroid/content/Context;",
            &[],
        )?
        .l()?;
    if application.is_null() || !env.is_instance_of(&application, "android/app/Application")? {
        return Err(invalid("Android 应用上下文不可用。"));
    }
    if let Some(existing) = CONTEXT.get() {
        return if env.is_same_object(existing.application.as_obj(), &application)? {
            Ok(())
        } else {
            Err(invalid("Android 应用上下文已经初始化。"))
        };
    }
    let context = Context {
        vm: env.get_java_vm()?,
        application: env.new_global_ref(application)?,
    };
    CONTEXT
        .set(context)
        .map_err(|_| invalid("Android 应用上下文初始化冲突。"))
}

/// Application.onCreate 在 NshView 启动 Rust 应用前调用。
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_manonloki_lokimetrisdisplay_bridge_DisplayApplication_nativeInit(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    application: JObject<'_>,
) {
    if let Err(Failure(message)) = install(&mut env, &application) {
        let _ = env.exception_clear();
        let _ = env.throw_new("java/lang/IllegalStateException", message);
    }
}

/// 每次调用建立有界本地引用帧；线程退出此作用域时释放 attach guard。
pub(super) fn with_context<T>(
    callback: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> Result<T>,
) -> Result<T> {
    let context = CONTEXT
        .get()
        .ok_or_else(|| invalid("Android 应用上下文尚未初始化，请重新打开应用。"))?;
    let mut env = context.vm.attach_current_thread()?;
    let result = env.with_local_frame(64, |env| callback(env, context.application.as_obj()));
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
        return Err(invalid("Android 系统调用失败，请重新打开应用。"));
    }
    result
}
