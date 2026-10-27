//! Existing Android Keystore envelope, accessed directly from the NSH Rust runtime.
use super::super::android_key_record::{self, Envelope};
use super::jni_support::{self as support, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use jni::{
    JNIEnv,
    objects::{JByteArray, JObject, JValue},
};

const ALIAS: &str = "loki-metris-rust-vault-v1";
const AAD: &[u8] = b"LokiMetris/rust-vault-key/1";

enum KeyAccess {
    Existing,
    Create,
}

struct Secret(Vec<u8>);

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

fn wrapping_key<'local>(env: &mut JNIEnv<'local>, access: KeyAccess) -> Result<JObject<'local>> {
    let provider = support::string(env, "AndroidKeyStore")?;
    let store = env
        .call_static_method(
            "java/security/KeyStore",
            "getInstance",
            "(Ljava/lang/String;)Ljava/security/KeyStore;",
            &[JValue::Object(&provider)],
        )?
        .l()?;
    env.call_method(
        &store,
        "load",
        "(Ljava/security/KeyStore$LoadStoreParameter;)V",
        &[JValue::Object(&JObject::null())],
    )?;
    let alias = support::string(env, ALIAS)?;
    let existing = env
        .call_method(
            &store,
            "getKey",
            "(Ljava/lang/String;[C)Ljava/security/Key;",
            &[JValue::Object(&alias), JValue::Object(&JObject::null())],
        )?
        .l()?;
    if !existing.is_null() {
        return Ok(existing);
    }
    if matches!(access, KeyAccess::Existing) {
        return Err(support::invalid(
            "Android 安全存储密钥不可用，请解锁设备后重试。",
        ));
    }
    let algorithm = support::string(env, "AES")?;
    let generator = env
        .call_static_method(
            "javax/crypto/KeyGenerator",
            "getInstance",
            "(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/KeyGenerator;",
            &[JValue::Object(&algorithm), JValue::Object(&provider)],
        )?
        .l()?;
    let builder = env.new_object(
        "android/security/keystore/KeyGenParameterSpec$Builder",
        "(Ljava/lang/String;I)V",
        &[JValue::Object(&alias), JValue::Int(3)],
    )?;
    let string_class = env.find_class("java/lang/String")?;
    let modes = env.new_object_array(1, &string_class, JObject::null())?;
    let gcm = support::string(env, "GCM")?;
    env.set_object_array_element(&modes, 0, gcm)?;
    env.call_method(
        &builder,
        "setBlockModes",
        "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
        &[JValue::Object(modes.as_ref())],
    )?;
    let paddings = env.new_object_array(1, string_class, JObject::null())?;
    let no_padding = support::string(env, "NoPadding")?;
    env.set_object_array_element(&paddings, 0, no_padding)?;
    env.call_method(
        &builder,
        "setEncryptionPaddings",
        "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
        &[JValue::Object(paddings.as_ref())],
    )?;
    env.call_method(
        &builder,
        "setKeySize",
        "(I)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
        &[JValue::Int(256)],
    )?;
    let spec = env
        .call_method(
            builder,
            "build",
            "()Landroid/security/keystore/KeyGenParameterSpec;",
            &[],
        )?
        .l()?;
    env.call_method(
        &generator,
        "init",
        "(Ljava/security/spec/AlgorithmParameterSpec;)V",
        &[JValue::Object(&spec)],
    )?;
    Ok(env
        .call_method(generator, "generateKey", "()Ljavax/crypto/SecretKey;", &[])?
        .l()?)
}

fn cipher<'local>(env: &mut JNIEnv<'local>) -> Result<JObject<'local>> {
    let algorithm = support::string(env, "AES/GCM/NoPadding")?;
    Ok(env
        .call_static_method(
            "javax/crypto/Cipher",
            "getInstance",
            "(Ljava/lang/String;)Ljavax/crypto/Cipher;",
            &[JValue::Object(&algorithm)],
        )?
        .l()?)
}

fn aad(env: &mut JNIEnv<'_>, cipher: &JObject<'_>) -> Result<()> {
    let aad = env.byte_array_from_slice(AAD)?;
    env.call_method(
        cipher,
        "updateAAD",
        "([B)V",
        &[JValue::Object(aad.as_ref())],
    )?;
    Ok(())
}

fn finish(env: &mut JNIEnv<'_>, cipher: &JObject<'_>, bytes: &[u8]) -> Result<Vec<u8>> {
    let input = env.byte_array_from_slice(bytes)?;
    let result = env
        .call_method(
            cipher,
            "doFinal",
            "([B)[B",
            &[JValue::Object(input.as_ref())],
        )?
        .l()?;
    let result = JByteArray::from(result);
    let output = env.convert_byte_array(&result)?;
    env.set_byte_array_region(&input, 0, &vec![0; bytes.len()])?;
    env.set_byte_array_region(&result, 0, &vec![0; output.len()])?;
    Ok(output)
}

pub(super) fn load(
    env: &mut JNIEnv<'_>,
    context: &JObject<'_>,
    vault: &std::path::Path,
) -> Result<[u8; 32]> {
    let prefs = support::preferences(env, context, "rust_vault")?;
    let iv = support::pref_string(env, &prefs, "iv")?;
    let wrapped = support::pref_string(env, &prefs, "wrapped")?;
    let cipher = cipher(env)?;
    let plain = match android_key_record::read(iv.as_deref(), wrapped.as_deref(), vault)
        .map_err(support::invalid)?
    {
        Envelope::Existing { iv, sealed } => {
            let key = wrapping_key(env, KeyAccess::Existing)?;
            let iv = env.byte_array_from_slice(&iv)?;
            let spec = env.new_object(
                "javax/crypto/spec/GCMParameterSpec",
                "(I[B)V",
                &[JValue::Int(128), JValue::Object(iv.as_ref())],
            )?;
            env.call_method(
                &cipher,
                "init",
                "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V",
                &[JValue::Int(2), JValue::Object(&key), JValue::Object(&spec)],
            )?;
            aad(env, &cipher)?;
            Secret(finish(env, &cipher, &sealed)?)
        }
        Envelope::New => {
            let random = env.new_object("java/security/SecureRandom", "()V", &[])?;
            let bytes = env.new_byte_array(32)?;
            env.call_method(
                random,
                "nextBytes",
                "([B)V",
                &[JValue::Object(bytes.as_ref())],
            )?;
            let fresh = Secret(env.convert_byte_array(&bytes)?);
            env.set_byte_array_region(&bytes, 0, &[0; 32])?;
            let key = wrapping_key(env, KeyAccess::Create)?;
            env.call_method(
                &cipher,
                "init",
                "(ILjava/security/Key;)V",
                &[JValue::Int(1), JValue::Object(&key)],
            )?;
            aad(env, &cipher)?;
            let sealed = finish(env, &cipher, &fresh.0)?;
            let iv = env.call_method(&cipher, "getIV", "()[B", &[])?.l()?;
            let iv = env.convert_byte_array(JByteArray::from(iv))?;
            let saved = support::save_strings(
                env,
                &prefs,
                &[
                    ("iv", &STANDARD.encode(iv)),
                    ("wrapped", &STANDARD.encode(sealed)),
                ],
            );
            saved?;
            fresh
        }
    };
    plain
        .0
        .as_slice()
        .try_into()
        .map_err(|_| support::invalid("Android 安全存储密钥长度无效。"))
}
