use super::*;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("loki-watch-staging-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("assets")).expect("create test asset directory");
        Self(root)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn queued_copy_survives_replaced_or_pruned_source() {
    let directory = Directory::new();
    let bytes = include_bytes!("../../../tests/fixtures/assets/hook.png");
    let descriptor = AssetDescriptor::from_bytes(bytes).expect("valid fixture PNG");
    let original = directory.0.join("assets").join(&descriptor.hash);
    fs::write(&original, bytes).expect("write fixture source");
    let staged = stage(&directory.0, &descriptor).expect("stage verified PNG");
    fs::write(&original, b"source replaced").expect("replace source");
    fs::remove_file(&original).expect("prune source");
    assert_eq!(fs::read(&staged).expect("immutable staging copy"), bytes);
    remove(&directory.0, &staged);
    assert!(!staged.exists());
}

#[test]
fn corrupt_content_is_not_queued_and_cleanup_only_removes_owned_staging() {
    let directory = Directory::new();
    let bytes = include_bytes!("../../../tests/fixtures/assets/hook.png");
    let descriptor = AssetDescriptor::from_bytes(bytes).expect("valid fixture PNG");
    let mut corrupt = bytes.to_vec();
    corrupt[40] ^= 1;
    fs::write(directory.0.join("assets").join(&descriptor.hash), corrupt)
        .expect("write corrupt fixture");
    assert!(stage(&directory.0, &descriptor).is_err());
    let unrelated = directory.0.join(format!("asset-{}", uuid::Uuid::new_v4()));
    fs::write(&unrelated, b"keep").expect("write unrelated fixture");
    remove(&directory.0, &unrelated);
    assert!(unrelated.exists());
}
