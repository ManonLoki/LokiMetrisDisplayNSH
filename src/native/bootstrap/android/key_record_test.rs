// 引入被测模块
use super::*;

#[test]
// 保险库已有数据却缺少密钥记录时，保留原数据并报错
fn an_existing_vault_without_its_envelope_is_preserved() {
    let directory =
        std::env::temp_dir().join(format!("loki-vault-record-{}", uuid::Uuid::new_v4()));
    // 保险库目录里已有数据
    fs::create_dir(&directory).unwrap();
    let file = directory.join("paired-device");
    fs::write(&file, b"existing ciphertext").unwrap();
    // 没有密钥记录：必须报错，不能新建密钥
    assert!(read(None, None, &directory).is_err());
    // 原数据完好
    assert_eq!(fs::read(&file).unwrap(), b"existing ciphertext");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
// 密钥记录不完整或损坏时，不会悄悄创建替代密钥
fn partial_or_corrupt_envelopes_cannot_create_a_replacement_key() {
    // 一个不存在的保险库目录
    let absent = std::env::temp_dir().join(format!("loki-vault-absent-{}", uuid::Uuid::new_v4()));
    let iv = STANDARD.encode([4; 12]);
    let wrapped = STANDARD.encode([5; 48]);
    // 只有一项、或任一项损坏，都应报错
    for (iv, sealed) in [
        (Some(iv.as_str()), None),
        (None, Some(wrapped.as_str())),
        (Some("broken"), Some(wrapped.as_str())),
        (Some(iv.as_str()), Some("broken")),
    ] {
        assert!(read(iv, sealed, &absent).is_err());
    }
    // 且不会创建目录
    assert!(!absent.exists());
}

#[test]
// 原始 base64 密钥记录能原样解码，不改动密文
fn original_base64_envelope_is_decoded_without_changing_ciphertext() {
    let iv = [7; 12];
    let sealed = vec![9; 48];
    // 合法记录被原样解码为 Existing
    assert_eq!(
        read(
            Some(&STANDARD.encode(iv)),
            Some(&STANDARD.encode(&sealed)),
            Path::new("unused")
        )
        .unwrap(),
        Envelope::Existing { iv, sealed }
    );
}

#[test]
// 只有保险库不存在或为空才允许首次创建
fn only_a_missing_or_empty_vault_allows_initial_creation() {
    let directory = std::env::temp_dir().join(format!("loki-vault-empty-{}", uuid::Uuid::new_v4()));
    // 目录不存在：允许首次创建
    assert_eq!(read(None, None, &directory).unwrap(), Envelope::New);
    fs::create_dir(&directory).unwrap();
    // 目录为空：同样允许
    assert_eq!(read(None, None, &directory).unwrap(), Envelope::New);
    fs::remove_dir(directory).unwrap();
}
