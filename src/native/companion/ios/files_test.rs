// 引入被测模块
use super::*;

// 临时目录，用完删除
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("loki-watch-staging-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("assets")).expect("create test asset directory");
        Self(root)
    }
}
// 临时目录的析构
impl Drop for Directory {
    // 销毁时
    fn drop(&mut self) {
        // 删除整个目录
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
// 已排队的副本不受来源被替换或清理的影响
fn queued_copy_survives_replaced_or_pruned_source() {
    let directory = Directory::new();
    // 用测试 PNG 作为图片
    let bytes = include_bytes!("../../../tests/fixtures/assets/hook.png");
    let descriptor = AssetDescriptor::from_bytes(bytes).expect("valid fixture PNG");
    // 缓存里的原图
    let original = directory.0.join("assets").join(&descriptor.hash);
    fs::write(&original, bytes).expect("write fixture source");
    // 暂存一份已校验的副本
    let staged = stage(&directory.0, &descriptor).expect("stage verified PNG");
    // 原图被替换、又被清理
    fs::write(&original, b"source replaced").expect("replace source");
    fs::remove_file(&original).expect("prune source");
    // 暂存副本仍是原内容
    assert_eq!(fs::read(&staged).expect("immutable staging copy"), bytes);
    // 清理暂存文件
    remove(&directory.0, &staged);
    assert!(!staged.exists());
}

#[test]
// 损坏的内容不会入队，清理只删除自己的暂存文件
fn corrupt_content_is_not_queued_and_cleanup_only_removes_owned_staging() {
    let directory = Directory::new();
    let bytes = include_bytes!("../../../tests/fixtures/assets/hook.png");
    let descriptor = AssetDescriptor::from_bytes(bytes).expect("valid fixture PNG");
    // 把缓存里的图片改坏一位
    let mut corrupt = bytes.to_vec();
    corrupt[40] ^= 1;
    fs::write(directory.0.join("assets").join(&descriptor.hash), corrupt)
        .expect("write corrupt fixture");
    // 摘要不符，不能入队
    assert!(stage(&directory.0, &descriptor).is_err());
    // 一个不在 staging 目录里的同名风格文件
    let unrelated = directory.0.join(format!("asset-{}", uuid::Uuid::new_v4()));
    fs::write(&unrelated, b"keep").expect("write unrelated fixture");
    // 清理函数不会删除不属于自己的文件
    remove(&directory.0, &unrelated);
    assert!(unrelated.exists());
}
