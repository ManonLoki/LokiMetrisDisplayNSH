//! 应用进程拥有 JNI 上下文；不保存 Activity，也不读取 SDK 的全局状态。
use super::jni_support::{Failure, Result, invalid};
use jni::{
    // JNI 环境与虚拟机
    JNIEnv,
    JavaVM,
    // 全局引用、类与对象
    objects::{GlobalRef, JClass, JObject},
};
use std::sync::OnceLock;

// JNI 上下文：虚拟机与 Application 的全局引用
struct Context {
    vm: JavaVM,
    application: GlobalRef,
}

// 全局上下文，只设置一次
static CONTEXT: OnceLock<Context> = OnceLock::new();

fn install(env: &mut JNIEnv<'_>, application: &JObject<'_>) -> Result<()> {
    // 取 ApplicationContext
    let application = env
        .call_method(
            application,
            // Java 类名或方法名参数
            "getApplicationContext",
            "()Landroid/content/Context;",
            // 传给 Java 方法的参数
            &[],
        )?
        .l()?;
    // 必须是 Application 实例
    if application.is_null() || !env.is_instance_of(&application, "android/app/Application")? {
        return Err(invalid("Android 应用上下文不可用。"));
    }
    // 已初始化：同一对象视为成功，不同对象报冲突
    if let Some(existing) = CONTEXT.get() {
        // 已初始化过：是同一个应用则成功
        return if env.is_same_object(existing.application.as_obj(), &application)? {
            Ok(())
        // 否则分支
        } else {
            // 否则报已经初始化
            Err(invalid("Android 应用上下文已经初始化。"))
        };
    }
    let context = Context {
        vm: env.get_java_vm()?,
        // 创建应用对象的全局引用
        application: env.new_global_ref(application)?,
    };
    CONTEXT
        // 只允许设置一次
        .set(context)
        .map_err(|_| invalid("Android 应用上下文初始化冲突。"))
}

/// Application.onCreate 在 NshView 启动 Rust 应用前调用。
// 导出给 Java 的 JNI 入口
#[unsafe(no_mangle)]
// Application.onCreate 在 NshView 启动 Rust 应用前调用
pub extern "system" fn Java_com_manonloki_lokimetrisdisplay_bridge_DisplayApplication_nativeInit(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    application: JObject<'_>,
) {
    // 失败则清除异常并抛出 IllegalStateException
    if let Err(Failure(message)) = install(&mut env, &application) {
        let _ = env.exception_clear();
        let _ = env.throw_new("java/lang/IllegalStateException", message);
    }
}

/// 每次调用建立有界本地引用帧；线程退出此作用域时释放 attach guard。
// 每次调用建立有界本地引用帧；线程退出此作用域时释放 attach guard
pub(super) fn with_context<T>(
    callback: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> Result<T>,
) -> Result<T> {
    let context = CONTEXT
        // 取已初始化的上下文
        .get()
        .ok_or_else(|| invalid("Android 应用上下文尚未初始化，请重新打开应用。"))?;
    // 附加当前线程
    let mut env = context.vm.attach_current_thread()?;
    // 在局部引用帧内执行回调
    let result = env.with_local_frame(64, |env| callback(env, context.application.as_obj()));
    // 有未处理的 Java 异常则清除并报错
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
        return Err(invalid("Android 系统调用失败，请重新打开应用。"));
    }
    result
}
