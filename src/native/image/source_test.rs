// 引入被测模块
use super::*;

// 临时目录，用完删除
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("loki-gif-source-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    // 拼出文件 URL
    fn url(&self, name: &str) -> String {
        format!("file://{}", self.0.join(name).display())
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
// 来源只能是有界的本地文件或合法的 base64 GIF
fn sources_are_bounded_local_files_or_valid_base64_gifs() {
    let root = Directory::new();
    // 文件名含空格，URL 里写成 %20
    fs::write(root.0.join("space image.gif"), b"GIF89a").unwrap();
    // 本地文件可读，不是内联 GIF
    assert_eq!(load(&root.url("space%20image.gif")).unwrap().0, b"GIF89a");
    // 内联 base64 GIF 同样可读，并标记为内联
    let inline = format!("data:image/gif;base64,{}", STANDARD.encode(b"GIF89a"));
    assert_eq!(load(&inline).unwrap(), (b"GIF89a".to_vec(), true));
    // 不受信任的来源：远程地址、带主机名、带查询或片段、非法转义、空字符、含 .. 、非法 base64
    for invalid in [
        "https://example.com/a.gif",
        "file://localhost/a",
        "file:///tmp/a?x=1",
        "file:///tmp/a#x",
        "file:///tmp/%gg",
        "file:///tmp/%00",
        "file:///tmp/../a",
        // 非法的 data URL
        "data:image/gif;base64,!",
        // 逐项检查
    ] {
        // 都应被拒绝
        assert!(load(invalid).is_err(), "untrusted source {invalid}");
    }
    // 超过大小上限的文件
    let large = fs::File::create(root.0.join("large.gif")).unwrap();
    large.set_len(MAX_SOURCE as u64 + 1).unwrap();
    assert!(load(&root.url("large.gif")).is_err());
    // 目录不是普通文件
    assert!(load(&format!("file://{}", root.0.display())).is_err());
    // 编码长度超限的内联 GIF
    assert!(
        load(&format!(
            "data:image/gif;base64,{}",
            "A".repeat(MAX_SOURCE.div_ceil(3) * 4 + 4)
        ))
        .is_err()
    );
}

#[cfg(unix)]
#[test]
// 符号链接的文件及其上级目录都被拒绝，且不触碰原数据
fn symlink_files_and_ancestors_are_rejected_without_touching_original_data() {
    use std::os::unix::fs::symlink;
    let root = Directory::new();
    fs::write(root.0.join("source.gif"), b"GIF89a").unwrap();
    // 文件本身是符号链接：拒绝
    symlink(root.0.join("source.gif"), root.0.join("alias.gif")).unwrap();
    assert!(load(&root.url("alias.gif")).is_err());
    // 上级目录是符号链接：同样拒绝
    fs::create_dir(root.0.join("real")).unwrap();
    fs::write(root.0.join("real/image.gif"), b"GIF89a").unwrap();
    symlink(root.0.join("real"), root.0.join("linked")).unwrap();
    assert!(load(&root.url("linked/image.gif")).is_err());
    // 原数据未被触碰
    assert_eq!(fs::read(root.0.join("source.gif")).unwrap(), b"GIF89a");
}
