use super::super::{connection_test::files, persistence};
use super::*;

fn image() -> Vec<u8> {
    b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b".to_vec()
}

#[test]
fn image_cache_rechecks_replacement_length_magic_and_hash() {
    let files = files();
    let directory = files.root.join("assets");
    fs::create_dir_all(&directory).unwrap();
    let bytes = image();
    let descriptor = AssetDescriptor::from_bytes(&bytes).unwrap();
    let path = directory.join(&descriptor.hash);
    persistence::atomic_write(&path, &bytes).unwrap();
    assert!(verify_file(&path, &descriptor).is_ok());
    let mut corrupt = bytes.clone();
    corrupt[10] ^= 1;
    persistence::atomic_write(&path, &corrupt).unwrap();
    assert!(verify_file(&path, &descriptor).is_err());
    persistence::atomic_write(&path, &bytes).unwrap();
    assert!(verify_file(&path, &descriptor).is_ok());
    fs::write(&path, b"GIF89a").unwrap();
    assert!(verify_file(&path, &descriptor).is_err());
}

#[cfg(unix)]
#[test]
fn image_cache_rejects_leaf_and_directory_symlinks() {
    use std::os::unix::fs::symlink;
    let files = files();
    let directory = files.root.join("assets");
    fs::create_dir_all(&directory).unwrap();
    let bytes = image();
    let descriptor = AssetDescriptor::from_bytes(&bytes).unwrap();
    let outside = files.root.join("outside");
    fs::write(&outside, &bytes).unwrap();
    let path = directory.join(&descriptor.hash);
    symlink(&outside, &path).unwrap();
    assert!(verify_file(&path, &descriptor).is_err());
    fs::remove_dir_all(&directory).unwrap();
    let external = files.root.join("external-assets");
    fs::create_dir(&external).unwrap();
    fs::write(external.join(&descriptor.hash), &bytes).unwrap();
    symlink(&external, &directory).unwrap();
    assert!(verify_file(&path, &descriptor).is_err());
    assert_eq!(fs::read(&outside).unwrap(), bytes);
}
