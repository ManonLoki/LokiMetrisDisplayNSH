//! 应用进程拥有 JNI 上下文；不保存 Activity，也不读取 SDK 的全局状态。
// 引入失败类型与辅助
use super::jni_support::{Failure, Result, invalid};
// 引入 JNI 类型
use jni::{
    // JNI 环境与虚拟机
    JNIEnv,
    // 虚拟机
    JavaVM,
    // 全局引用、类与对象
    objects::{GlobalRef, JClass, JObject},
};
// 引入一次性初始化
use std::sync::OnceLock;

// JNI 上下文：虚拟机与 Application 的全局引用
struct Context {
    // 虚拟机
    vm: JavaVM,
    // 应用对象
    application: GlobalRef,
}

// 全局上下文，只设置一次
static CONTEXT: OnceLock<Context> = OnceLock::new();

// 保存应用上下文
fn install(env: &mut JNIEnv<'_>, application: &JObject<'_>) -> Result<()> {
    // 取 ApplicationContext
    let application = env
        // 调用方法
        .call_method(
            // 应用对象
            application,
            // Java 类名或方法名参数
            "getApplicationContext",
            // JNI 方法签名
            "()Landroid/content/Context;",
            // 传给 Java 方法的参数
            &[],
            // 参数结束
        )?
        // 取回返回值
        .l()?;
    // 必须是 Application 实例
    if application.is_null() || !env.is_instance_of(&application, "android/app/Application")? {
        // 应用上下文不可用
        return Err(invalid("Android 应用上下文不可用。"));
    }
    // 已初始化：同一对象视为成功，不同对象报冲突
    if let Some(existing) = CONTEXT.get() {
        // 已初始化过：是同一个应用则成功
        return if env.is_same_object(existing.application.as_obj(), &application)? {
            // 成功返回
            Ok(())
        // 否则分支
        } else {
            // 否则报已经初始化
            Err(invalid("Android 应用上下文已经初始化。"))
        };
    }
    // 构造上下文
    let context = Context {
        // 取虚拟机
        vm: env.get_java_vm()?,
        // 创建应用对象的全局引用
        application: env.new_global_ref(application)?,
    };
    // 全局上下文
    CONTEXT
        // 只允许设置一次
        .set(context)
        // 重复设置则报冲突
        .map_err(|_| invalid("Android 应用上下文初始化冲突。"))
}

/// Application.onCreate 在 NshView 启动 Rust 应用前调用。
// 导出给 Java 的 JNI 入口
#[unsafe(no_mangle)]
// Application.onCreate 在 NshView 启动 Rust 应用前调用
pub extern "system" fn Java_com_manonloki_lokimetrisdisplay_bridge_DisplayApplication_nativeInit(
    // JNI 环境
    mut env: JNIEnv<'_>,
    // 类（未使用）
    _class: JClass<'_>,
    // 应用对象
    application: JObject<'_>,
    // 方法体
) {
    // 失败则清除异常并抛出 IllegalStateException
    if let Err(Failure(message)) = install(&mut env, &application) {
        // 清除待处理异常
        let _ = env.exception_clear();
        // 抛出状态异常
        let _ = env.throw_new("java/lang/IllegalStateException", message);
    }
}

/// 每次调用建立有界本地引用帧；线程退出此作用域时释放 attach guard。
// 每次调用建立有界本地引用帧；线程退出此作用域时释放 attach guard
pub(super) fn with_context<T>(
    // 回调
    callback: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> Result<T>,
    // 返回结果
) -> Result<T> {
    // 取全局上下文
    let context = CONTEXT
        // 取已初始化的上下文
        .get()
        // 未初始化则报错
        .ok_or_else(|| invalid("Android 应用上下文尚未初始化，请重新打开应用。"))?;
    // 附加当前线程
    let mut env = context.vm.attach_current_thread()?;
    // 在局部引用帧内执行回调
    let result = env.with_local_frame(64, |env| callback(env, context.application.as_obj()));
    // 有未处理的 Java 异常则清除并报错
    if env.exception_check().unwrap_or(false) {
        // 清除待处理异常
        let _ = env.exception_clear();
        // 报系统调用失败
        return Err(invalid("Android 系统调用失败，请重新打开应用。"));
    }
    // 返回结果
    result
}
