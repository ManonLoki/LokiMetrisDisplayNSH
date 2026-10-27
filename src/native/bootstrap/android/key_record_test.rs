use super::*;

#[test]
fn an_existing_vault_without_its_envelope_is_preserved() {
    let directory =
        std::env::temp_dir().join(format!("loki-vault-record-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let file = directory.join("paired-device");
    fs::write(&file, b"existing ciphertext").unwrap();
    assert!(read(None, None, &directory).is_err());
    assert_eq!(fs::read(&file).unwrap(), b"existing ciphertext");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn partial_or_corrupt_envelopes_cannot_create_a_replacement_key() {
    let absent = std::env::temp_dir().join(format!("loki-vault-absent-{}", uuid::Uuid::new_v4()));
    let iv = STANDARD.encode([4; 12]);
    let wrapped = STANDARD.encode([5; 48]);
    for (iv, sealed) in [
        (Some(iv.as_str()), None),
        (None, Some(wrapped.as_str())),
        (Some("broken"), Some(wrapped.as_str())),
        (Some(iv.as_str()), Some("broken")),
    ] {
        assert!(read(iv, sealed, &absent).is_err());
    }
    assert!(!absent.exists());
}

#[test]
fn original_base64_envelope_is_decoded_without_changing_ciphertext() {
    let iv = [7; 12];
    let sealed = vec![9; 48];
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
fn only_a_missing_or_empty_vault_allows_initial_creation() {
    let directory = std::env::temp_dir().join(format!("loki-vault-empty-{}", uuid::Uuid::new_v4()));
    assert_eq!(read(None, None, &directory).unwrap(), Envelope::New);
    fs::create_dir(&directory).unwrap();
    assert_eq!(read(None, None, &directory).unwrap(), Envelope::New);
    fs::remove_dir(directory).unwrap();
}
