//! 现有的 Android Keystore 信封,由 NSH Rust 运行时直接访问。
// 引入密钥记录校验
use super::super::android_key_record::{self, Envelope};
// 引入 JNI 辅助
use super::jni_support::{self as support, Result};
// 引入 base64
use base64::{Engine, engine::general_purpose::STANDARD};
// 引入 JNI 类型
use jni::{
    // JNI 环境
    JNIEnv,
    // 字节数组与对象类型
    objects::{JByteArray, JObject, JValue},
};

// Keystore 中包裹密钥的别名
const ALIAS: &str = "loki-metris-rust-vault-v1";
// AES-GCM 的关联数据
const AAD: &[u8] = b"LokiMetris/rust-vault-key/1";

// 包裹密钥的访问方式：仅取已有，或在缺失时创建
enum KeyAccess {
    // 已有密钥
    Existing,
    // 创建
    Create,
}

// 密钥字节，释放时清零
struct Secret(Vec<u8>);

// Secret 的析构
impl Drop for Secret {
    // 销毁时
    fn drop(&mut self) {
        // 把密钥字节清零
        self.0.fill(0);
    }
}

// 取得 Keystore 里的 AES 包裹密钥；缺失时按需创建
fn wrapping_key<'local>(env: &mut JNIEnv<'local>, access: KeyAccess) -> Result<JObject<'local>> {
    // 密钥库提供者名
    let provider = support::string(env, "AndroidKeyStore")?;
    // 获取并加载 AndroidKeyStore
    let store = env
        // 调用静态方法
        .call_static_method(
            // Java 类名或方法名参数
            "java/security/KeyStore",
            // Java 类名或方法名参数
            "getInstance",
            // JNI 方法签名
            "(Ljava/lang/String;)Ljava/security/KeyStore;",
            // 传给 Java 方法的参数
            &[JValue::Object(&provider)],
            // 参数结束
        )?
        // 取回返回值
        .l()?;
    // 调用方法
    env.call_method(
        // 密钥库
        &store,
        // Java 类名或方法名参数
        "load",
        // JNI 方法签名
        "(Ljava/security/KeyStore$LoadStoreParameter;)V",
        // 传给 Java 方法的参数
        &[JValue::Object(&JObject::null())],
        // 调用结束，传播 JNI 错误
    )?;
    // 密钥别名
    let alias = support::string(env, ALIAS)?;
    // 先找已有的密钥
    let existing = env
        // 调用方法
        .call_method(
            // 密钥库
            &store,
            // Java 类名或方法名参数
            "getKey",
            // JNI 方法签名
            "(Ljava/lang/String;[C)Ljava/security/Key;",
            // 传给 Java 方法的参数
            &[JValue::Object(&alias), JValue::Object(&JObject::null())],
            // 参数结束
        )?
        // 取回返回值
        .l()?;
    // 已有密钥
    if !existing.is_null() {
        // 直接返回
        return Ok(existing);
    }
    // 只允许读取已有时，找不到就失败，不悄悄新建
    if matches!(access, KeyAccess::Existing) {
        // 返回无效错误
        return Err(support::invalid(
            // 提示解锁设备后重试
            "Android 安全存储密钥不可用，请解锁设备后重试。",
        ));
    }
    // 创建 AES 密钥生成器
    let algorithm = support::string(env, "AES")?;
    // 密钥生成器
    let generator = env
        // 调用静态方法
        .call_static_method(
            // Java 类名或方法名参数
            "javax/crypto/KeyGenerator",
            // Java 类名或方法名参数
            "getInstance",
            // JNI 方法签名
            "(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/KeyGenerator;",
            // 传给 Java 方法的参数
            &[JValue::Object(&algorithm), JValue::Object(&provider)],
            // 参数结束
        )?
        // 取回返回值
        .l()?;
    // 密钥参数：用途为加解密（3）
    let builder = env.new_object(
        // Java 类名或方法名参数
        "android/security/keystore/KeyGenParameterSpec$Builder",
        // JNI 方法签名
        "(Ljava/lang/String;I)V",
        // 传给 Java 方法的参数
        &[JValue::Object(&alias), JValue::Int(3)],
        // 调用结束，传播 JNI 错误
    )?;
    // 分组模式只允许 GCM
    let string_class = env.find_class("java/lang/String")?;
    // 分组模式数组
    let modes = env.new_object_array(1, &string_class, JObject::null())?;
    // GCM 模式
    let gcm = support::string(env, "GCM")?;
    // 放入数组
    env.set_object_array_element(&modes, 0, gcm)?;
    // 设置分组模式
    env.call_method(
        // 构建器
        &builder,
        // Java 类名或方法名参数
        "setBlockModes",
        // JNI 方法签名
        "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
        // 传给 Java 方法的参数
        &[JValue::Object(modes.as_ref())],
        // 调用结束，传播 JNI 错误
    )?;
    // 不使用填充
    let paddings = env.new_object_array(1, string_class, JObject::null())?;
    // 无填充
    let no_padding = support::string(env, "NoPadding")?;
    // 放入数组
    env.set_object_array_element(&paddings, 0, no_padding)?;
    // 设置填充方式
    env.call_method(
        // 构建器
        &builder,
        // Java 类名或方法名参数
        "setEncryptionPaddings",
        // JNI 方法签名
        "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
        // 传给 Java 方法的参数
        &[JValue::Object(paddings.as_ref())],
        // 调用结束，传播 JNI 错误
    )?;
    // 密钥长度 256 位
    env.call_method(
        // 构建器
        &builder,
        // Java 类名或方法名参数
        "setKeySize",
        // JNI 方法签名
        "(I)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
        // 传给 Java 方法的参数
        &[JValue::Int(256)],
        // 调用结束，传播 JNI 错误
    )?;
    // 构建密钥参数
    let spec = env
        // 调用方法
        .call_method(
            // 构建器
            builder,
            // Java 类名或方法名参数
            "build",
            // JNI 方法签名
            "()Landroid/security/keystore/KeyGenParameterSpec;",
            // 传给 Java 方法的参数
            &[],
            // 参数结束
        )?
        // 取回返回值
        .l()?;
    // 初始化生成器
    env.call_method(
        // 生成器
        &generator,
        // Java 类名或方法名参数
        "init",
        // JNI 方法签名
        "(Ljava/security/spec/AlgorithmParameterSpec;)V",
        // 传给 Java 方法的参数
        &[JValue::Object(&spec)],
        // 调用结束，传播 JNI 错误
    )?;
    // 生成密钥
    Ok(env
        // 生成密钥
        .call_method(generator, "generateKey", "()Ljavax/crypto/SecretKey;", &[])?
        // 取回返回值
        .l()?)
}

// 创建 AES/GCM/NoPadding 加密器
fn cipher<'local>(env: &mut JNIEnv<'local>) -> Result<JObject<'local>> {
    // 算法名
    let algorithm = support::string(env, "AES/GCM/NoPadding")?;
    // 创建加密器
    Ok(env
        // 调用静态方法
        .call_static_method(
            // Java 类名或方法名参数
            "javax/crypto/Cipher",
            // Java 类名或方法名参数
            "getInstance",
            // JNI 方法签名
            "(Ljava/lang/String;)Ljavax/crypto/Cipher;",
            // 传给 Java 方法的参数
            &[JValue::Object(&algorithm)],
            // 参数结束
        )?
        // 取回返回值
        .l()?)
}

// 写入关联数据
fn aad(env: &mut JNIEnv<'_>, cipher: &JObject<'_>) -> Result<()> {
    // 附加认证数据
    let aad = env.byte_array_from_slice(AAD)?;
    // 更新加密器
    env.call_method(
        // 加密器
        cipher,
        // Java 类名或方法名参数
        "updateAAD",
        // JNI 方法签名
        "([B)V",
        // 传给 Java 方法的参数
        &[JValue::Object(aad.as_ref())],
        // 调用结束，传播 JNI 错误
    )?;
    // 成功返回
    Ok(())
}

// 完成加解密，并在返回前把中间字节数组清零
fn finish(env: &mut JNIEnv<'_>, cipher: &JObject<'_>, bytes: &[u8]) -> Result<Vec<u8>> {
    // 输入字节
    let input = env.byte_array_from_slice(bytes)?;
    // 执行加解密
    let result = env
        // 调用方法
        .call_method(
            // 加密器
            cipher,
            // Java 类名或方法名参数
            "doFinal",
            // JNI 方法签名
            "([B)[B",
            // 传给 Java 方法的参数
            &[JValue::Object(input.as_ref())],
            // 参数结束
        )?
        // 取回返回值
        .l()?;
    // 转为字节数组
    let result = JByteArray::from(result);
    // 转为 Rust 字节
    let output = env.convert_byte_array(&result)?;
    // 清零输入
    env.set_byte_array_region(&input, 0, &vec![0; bytes.len()])?;
    // 清零输出的 Java 副本
    env.set_byte_array_region(&result, 0, &vec![0; output.len()])?;
    // 返回输出
    Ok(output)
}

// 加载保险库密钥：已有则用 Keystore 密钥解开，否则随机生成并加密保存
pub(super) fn load(
    // JNI 环境
    env: &mut JNIEnv<'_>,
    // 应用上下文
    context: &JObject<'_>,
    // 保险库路径
    vault: &std::path::Path,
    // 返回 32 字节密钥
) -> Result<[u8; 32]> {
    // 读取保险库偏好
    let prefs = support::preferences(env, context, "rust_vault")?;
    // 已保存的初始向量
    let iv = support::pref_string(env, &prefs, "iv")?;
    // 已保存的包装密钥
    let wrapped = support::pref_string(env, &prefs, "wrapped")?;
    // 取得加密器
    let cipher = cipher(env)?;
    // 先校验记录状态，失败关闭
    let plain = match android_key_record::read(iv.as_deref(), wrapped.as_deref(), vault)
        // 失败转为无效错误
        .map_err(support::invalid)?
    {
        // 已有记录：解密包裹的密钥
        Envelope::Existing { iv, sealed } => {
            // 取已有的包装密钥
            let key = wrapping_key(env, KeyAccess::Existing)?;
            // 初始向量转数组
            let iv = env.byte_array_from_slice(&iv)?;
            // 128 位认证标签
            let spec = env.new_object(
                // Java 类名或方法名参数
                "javax/crypto/spec/GCMParameterSpec",
                // JNI 方法签名
                "(I[B)V",
                // 传给 Java 方法的参数
                &[JValue::Int(128), JValue::Object(iv.as_ref())],
                // 调用结束，传播 JNI 错误
            )?;
            // 解密模式（2）
            env.call_method(
                // 加密器
                &cipher,
                // Java 类名或方法名参数
                "init",
                // JNI 方法签名
                "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V",
                // 传给 Java 方法的参数
                &[JValue::Int(2), JValue::Object(&key), JValue::Object(&spec)],
                // 调用结束，传播 JNI 错误
            )?;
            // 附加认证数据
            aad(env, &cipher)?;
            // 解出密钥
            Secret(finish(env, &cipher, &sealed)?)
        }
        // 全新安装：生成随机密钥
        Envelope::New => {
            // 安全随机数
            let random = env.new_object("java/security/SecureRandom", "()V", &[])?;
            // 32 字节缓冲区
            let bytes = env.new_byte_array(32)?;
            // 填充随机数
            env.call_method(
                // 随机数
                random,
                // Java 类名或方法名参数
                "nextBytes",
                // JNI 方法签名
                "([B)V",
                // 传给 Java 方法的参数
                &[JValue::Object(bytes.as_ref())],
                // 调用结束，传播 JNI 错误
            )?;
            // 新生成的密钥
            let fresh = Secret(env.convert_byte_array(&bytes)?);
            // 清零 Java 侧的副本
            env.set_byte_array_region(&bytes, 0, &[0; 32])?;
            // 创建包装密钥
            let key = wrapping_key(env, KeyAccess::Create)?;
            // 加密模式（1）
            env.call_method(
                // 加密器
                &cipher,
                // Java 类名或方法名参数
                "init",
                // JNI 方法签名
                "(ILjava/security/Key;)V",
                // 传给 Java 方法的参数
                &[JValue::Int(1), JValue::Object(&key)],
                // 调用结束，传播 JNI 错误
            )?;
            // 附加认证数据
            aad(env, &cipher)?;
            // 加密新密钥
            let sealed = finish(env, &cipher, &fresh.0)?;
            // 取加密器生成的 IV
            let iv = env.call_method(&cipher, "getIV", "()[B", &[])?.l()?;
            // 取出初始向量
            let iv = env.convert_byte_array(JByteArray::from(iv))?;
            // 把 IV 与被包裹的密钥保存到偏好
            let saved = support::save_strings(
                // JNI 环境
                env,
                // 偏好
                &prefs,
                // 写入项
                &[
                    // 初始向量
                    ("iv", &STANDARD.encode(iv)),
                    // 包装后的密钥
                    ("wrapped", &STANDARD.encode(sealed)),
                ],
            );
            // 先确认保存成功
            saved?;
            // 返回新生成的密钥
            fresh
        }
    };
    // 必须正好 32 字节
    plain
        // 取密钥字节
        .0
        // 转为切片
        .as_slice()
        // 转为定长数组
        .try_into()
        // 长度不对则报密钥长度无效
        .map_err(|_| support::invalid("Android 安全存储密钥长度无效。"))
}
