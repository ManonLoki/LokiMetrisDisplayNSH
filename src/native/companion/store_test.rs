use super::*;
use serde_json::json;

const SOURCE: &str = "c02d28a3-bede-46cd-a21e-daa576b7ee0c";

struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("loki-companion-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn outbox(&self) -> std::path::PathBuf {
        self.0.join("companion-v1")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn snapshot() -> Value {
    json!({
        "version":1, "tokens":{"day":{"date":"2026-10-27","zone":"Asia/Shanghai"},
            "all":{"total":"18446744073709551615","availability":"available",
                "completeness":"complete","confidence":"exact"},"agents":[]},
        "tokenError":null, "hooksAvailable":true, "hooks":[], "providerQuota":null,
        "enabledAgents":[], "assets":[], "secret":"must never leave phone", "readKey":"private",
    })
}

#[test]
fn public_projection_preserves_exact_tokens_and_removes_private_fields() {
    let temp = Temp::new();
    let raw = snapshot();
    assert!(update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap());
    let manifest = read_manifest(&temp.outbox()).unwrap();
    let projected = manifest.snapshot.as_ref().unwrap();
    assert_eq!(projected["tokens"]["all"]["total"], "18446744073709551615");
    assert_eq!(manifest.received_at_ms, Some(7));
    let bytes = fs::read(temp.outbox().join("manifest.json")).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("private") && !text.contains("secret") && !text.contains("readKey"));
    assert!(!update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap());
    assert_eq!(read_manifest(&temp.outbox()).unwrap().revision, "1");
}

#[test]
fn clear_is_durable_and_revision_never_resets_across_reopen_or_source_switch() {
    let temp = Temp::new();
    let raw = snapshot();
    update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap();
    let first = read_manifest(&temp.outbox()).unwrap();
    update(&temp.outbox(), &temp.0, None, None, None).unwrap();
    let clear = read_manifest(&temp.outbox()).unwrap();
    assert_eq!(clear.revision, "2");
    assert_eq!(clear.stream_id, first.stream_id);
    assert!(clear.snapshot.is_none() && clear.source_id.is_none() && clear.assets.is_empty());
    assert!(!update(&temp.outbox(), &temp.0, None, None, None).unwrap());
    update(
        &temp.outbox(),
        &temp.0,
        Some("388c3756-964a-4e8b-aa68-4b55fd3c1ce6"),
        Some(&raw),
        Some(8),
    )
    .unwrap();
    assert_eq!(read_manifest(&temp.outbox()).unwrap().revision, "3");
}

#[test]
fn images_are_checked_copied_when_ready_and_removed_on_clear() {
    let temp = Temp::new();
    let bytes = include_bytes!("../../assets/branding/app-icon.png");
    let descriptor = AssetDescriptor::from_bytes(bytes).unwrap();
    let mut raw = snapshot();
    raw["hooks"] = json!([{"agent":"codex","slot":1,"behavior":"running",
        "content":"working","contentTruncated":false,"image":descriptor.hash,
        "imageSource":"file:///private/phone.png"}]);
    raw["assets"] = json!([descriptor]);
    update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap();
    let initial = read_manifest(&temp.outbox()).unwrap();
    assert!(
        initial.snapshot.as_ref().unwrap()["hooks"][0]
            .get("imageSource")
            .is_none()
    );
    assert!(!temp.outbox().join("assets").join(&descriptor.hash).exists());
    directory(&temp.0.join("assets")).unwrap();
    fs::write(temp.0.join("assets").join(&descriptor.hash), b"corrupt").unwrap();
    assert!(!update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap());
    fs::write(temp.0.join("assets").join(&descriptor.hash), bytes).unwrap();
    assert!(update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap());
    let ready = read_manifest(&temp.outbox()).unwrap();
    let ready_bytes = fs::read(temp.outbox().join("manifest.json")).unwrap();
    assert_eq!(
        ready.revision, initial.revision,
        "asset completion does not invent a new snapshot"
    );
    assert_eq!(
        fs::read(temp.outbox().join("assets").join(&descriptor.hash)).unwrap(),
        bytes
    );
    update(&temp.outbox(), &temp.0, None, None, None).unwrap();
    assert!(!temp.outbox().join("assets").join(&descriptor.hash).exists());
    if let Some(directory) = std::env::var_os("LOKI_WATCH_INTEROP") {
        let directory = std::path::PathBuf::from(directory);
        assert!(
            directory.is_absolute(),
            "interop output must be an absolute test path"
        );
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("manifest.json"), ready_bytes).unwrap();
        fs::write(directory.join(&descriptor.hash), bytes).unwrap();
        fs::copy(
            temp.outbox().join("manifest.json"),
            directory.join("clear.json"),
        )
        .unwrap();
    }
}

#[test]
fn invalid_input_does_not_replace_a_valid_snapshot_or_overwrite_bad_state() {
    let temp = Temp::new();
    let raw = snapshot();
    update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap();
    let first = fs::read(temp.outbox().join("manifest.json")).unwrap();
    assert!(
        update(
            &temp.outbox(),
            &temp.0,
            Some("not-a-source"),
            Some(&raw),
            Some(7)
        )
        .is_err()
    );
    assert_eq!(
        fs::read(temp.outbox().join("manifest.json")).unwrap(),
        first
    );
    fs::write(temp.outbox().join("manifest.json"), b"broken").unwrap();
    assert!(update(&temp.outbox(), &temp.0, None, None, None).is_err());
    assert_eq!(
        fs::read(temp.outbox().join("manifest.json")).unwrap(),
        b"broken"
    );
}

#[test]
fn path_substitution_and_revision_overflow_fail_closed() {
    let temp = Temp::new();
    update(&temp.outbox(), &temp.0, None, None, None).unwrap();
    let mut manifest = read_manifest(&temp.outbox()).unwrap();
    manifest.revision = u64::MAX.to_string();
    fs::write(
        temp.outbox().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(
        update(
            &temp.outbox(),
            &temp.0,
            Some(SOURCE),
            Some(&snapshot()),
            Some(7)
        )
        .is_err()
    );
    #[cfg(unix)]
    {
        let victim = temp.0.join("victim.json");
        fs::write(&victim, b"untouched").unwrap();
        fs::remove_file(temp.outbox().join("manifest.json")).unwrap();
        std::os::unix::fs::symlink(&victim, temp.outbox().join("manifest.json")).unwrap();
        assert!(update(&temp.outbox(), &temp.0, None, None, None).is_err());
        assert_eq!(fs::read(victim).unwrap(), b"untouched");
    }
}
