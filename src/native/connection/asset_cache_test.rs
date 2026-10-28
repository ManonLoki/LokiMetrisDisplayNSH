// 引入测试文件助手
use super::super::{connection_test::files, persistence};
use super::*;

// 构造测试图片字节
fn image() -> Vec<u8> {
    // 一个最小的 1x1 GIF
    b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b".to_vec()
}

#[test]
// 缓存校验会重新检查被替换文件的长度、魔数和摘要
fn image_cache_rechecks_replacement_length_magic_and_hash() {
    let files = files();
    // 图片缓存目录
    let directory = files.root.join("assets");
    fs::create_dir_all(&directory).unwrap();
    let bytes = image();
    // 由原始字节生成描述（长度与摘要）
    let descriptor = AssetDescriptor::from_bytes(&bytes).unwrap();
    // 按摘要命名的缓存文件
    let path = directory.join(&descriptor.hash);
    persistence::atomic_write(&path, &bytes).unwrap();
    // 原始内容通过校验
    assert!(verify_file(&path, &descriptor).is_ok());
    // 翻转一位：摘要不符
    let mut corrupt = bytes.clone();
    corrupt[10] ^= 1;
    persistence::atomic_write(&path, &corrupt).unwrap();
    assert!(verify_file(&path, &descriptor).is_err());
    // 恢复原内容后重新通过
    persistence::atomic_write(&path, &bytes).unwrap();
    assert!(verify_file(&path, &descriptor).is_ok());
    // 长度被截短：失败
    fs::write(&path, b"GIF89a").unwrap();
    assert!(verify_file(&path, &descriptor).is_err());
}

#[cfg(unix)]
#[test]
// 缓存拒绝文件与目录的符号链接
fn image_cache_rejects_leaf_and_directory_symlinks() {
    use std::os::unix::fs::symlink;
    let files = files();
    let directory = files.root.join("assets");
    fs::create_dir_all(&directory).unwrap();
    let bytes = image();
    let descriptor = AssetDescriptor::from_bytes(&bytes).unwrap();
    // 缓存之外放一份相同内容的文件
    let outside = files.root.join("outside");
    fs::write(&outside, &bytes).unwrap();
    let path = directory.join(&descriptor.hash);
    // 缓存文件是指向外部的符号链接：拒绝
    symlink(&outside, &path).unwrap();
    assert!(verify_file(&path, &descriptor).is_err());
    // 换成目录级符号链接
    fs::remove_dir_all(&directory).unwrap();
    let external = files.root.join("external-assets");
    fs::create_dir(&external).unwrap();
    fs::write(external.join(&descriptor.hash), &bytes).unwrap();
    symlink(&external, &directory).unwrap();
    // 同样拒绝
    assert!(verify_file(&path, &descriptor).is_err());
    // 原数据未被触碰
    assert_eq!(fs::read(&outside).unwrap(), bytes);
}
