use super::*;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("loki-gif-source-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
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
fn sources_are_bounded_local_files_or_valid_base64_gifs() {
    let root = Directory::new();
    fs::write(root.0.join("space image.gif"), b"GIF89a").unwrap();
    assert_eq!(load(&root.url("space%20image.gif")).unwrap().0, b"GIF89a");
    let inline = format!("data:image/gif;base64,{}", STANDARD.encode(b"GIF89a"));
    assert_eq!(load(&inline).unwrap(), (b"GIF89a".to_vec(), true));
    for invalid in [
        "https://example.com/a.gif",
        "file://localhost/a",
        "file:///tmp/a?x=1",
        "file:///tmp/a#x",
        "file:///tmp/%gg",
        "file:///tmp/%00",
        "file:///tmp/../a",
        "data:image/gif;base64,!",
    ] {
        assert!(load(invalid).is_err(), "untrusted source {invalid}");
    }
    let large = fs::File::create(root.0.join("large.gif")).unwrap();
    large.set_len(MAX_SOURCE as u64 + 1).unwrap();
    assert!(load(&root.url("large.gif")).is_err());
    assert!(load(&format!("file://{}", root.0.display())).is_err());
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
fn symlink_files_and_ancestors_are_rejected_without_touching_original_data() {
    use std::os::unix::fs::symlink;
    let root = Directory::new();
    fs::write(root.0.join("source.gif"), b"GIF89a").unwrap();
    symlink(root.0.join("source.gif"), root.0.join("alias.gif")).unwrap();
    assert!(load(&root.url("alias.gif")).is_err());
    fs::create_dir(root.0.join("real")).unwrap();
    fs::write(root.0.join("real/image.gif"), b"GIF89a").unwrap();
    symlink(root.0.join("real"), root.0.join("linked")).unwrap();
    assert!(load(&root.url("linked/image.gif")).is_err());
    assert_eq!(fs::read(root.0.join("source.gif")).unwrap(), b"GIF89a");
}
