// 引入被测模块
use super::*;
use serde_json::json;

// 测试用来源 ID
const SOURCE: &str = "c02d28a3-bede-46cd-a21e-daa576b7ee0c";

// 临时目录，用完删除
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        // 随机的临时根目录
        let root = std::env::temp_dir().join(format!("loki-companion-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    // 发件箱目录
    fn outbox(&self) -> std::path::PathBuf {
        self.0.join("companion-v1")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// 构造测试快照
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
// 公开投影保留精确的 Token，并去掉私密字段
fn public_projection_preserves_exact_tokens_and_removes_private_fields() {
    let temp = Temp::new();
    let raw = snapshot();
    // 首次写入：有变化
    assert!(update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap());
    let manifest = read_manifest(&temp.outbox()).unwrap();
    let projected = manifest.snapshot.as_ref().unwrap();
    // u64 最大值保持精确
    assert_eq!(projected["tokens"]["all"]["total"], "18446744073709551615");
    assert_eq!(manifest.received_at_ms, Some(7));
    // 清单文本里不含任何私密字段
    let bytes = fs::read(temp.outbox().join("manifest.json")).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("private") && !text.contains("secret") && !text.contains("readKey"));
    // 内容相同则没有变化，修订号仍为 1
    assert!(!update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap());
    assert_eq!(read_manifest(&temp.outbox()).unwrap().revision, "1");
}

#[test]
// 清空是持久的；重新打开或切换来源后修订号绝不回退
fn clear_is_durable_and_revision_never_resets_across_reopen_or_source_switch() {
    let temp = Temp::new();
    let raw = snapshot();
    // 写入第一份快照
    update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap();
    let first = read_manifest(&temp.outbox()).unwrap();
    // 清空
    update(&temp.outbox(), &temp.0, None, None, None).unwrap();
    // 清空后修订号 +1，流 ID 不变，来源与快照都为空
    let clear = read_manifest(&temp.outbox()).unwrap();
    assert_eq!(clear.revision, "2");
    assert_eq!(clear.stream_id, first.stream_id);
    assert!(clear.snapshot.is_none() && clear.source_id.is_none() && clear.assets.is_empty());
    // 重复清空没有变化
    assert!(!update(&temp.outbox(), &temp.0, None, None, None).unwrap());
    // 切换到另一个来源
    update(
        &temp.outbox(),
        &temp.0,
        Some("388c3756-964a-4e8b-aa68-4b55fd3c1ce6"),
        Some(&raw),
        Some(8),
    )
    .unwrap();
    // 修订号继续递增，绝不回退
    assert_eq!(read_manifest(&temp.outbox()).unwrap().revision, "3");
}

#[test]
// 图片校验后在就绪时复制，清空时一并删除
fn images_are_checked_copied_when_ready_and_removed_on_clear() {
    let temp = Temp::new();
    // 用应用图标作为测试图片
    let bytes = include_bytes!("../../assets/branding/app-icon.png");
    let descriptor = AssetDescriptor::from_bytes(bytes).unwrap();
    // 快照里有一条引用该图片的 Hook，并带有本地地址
    let mut raw = snapshot();
    raw["hooks"] = json!([{"agent":"codex","slot":1,"behavior":"running",
        "content":"working","contentTruncated":false,"image":descriptor.hash,
        "imageSource":"file:///private/phone.png"}]);
    raw["assets"] = json!([descriptor]);
    // 此时图片还没在缓存里
    update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap();
    let initial = read_manifest(&temp.outbox()).unwrap();
    // 清单里的 Hook 不含 imageSource
    assert!(
        initial.snapshot.as_ref().unwrap()["hooks"][0]
            .get("imageSource")
            .is_none()
    );
    // 发件箱里还没有图片
    assert!(!temp.outbox().join("assets").join(&descriptor.hash).exists());
    // 放入一份损坏的图片：校验不通过，不复制
    directory(&temp.0.join("assets")).unwrap();
    fs::write(temp.0.join("assets").join(&descriptor.hash), b"corrupt").unwrap();
    assert!(!update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap());
    // 换成正确内容后才复制，算作有变化
    fs::write(temp.0.join("assets").join(&descriptor.hash), bytes).unwrap();
    assert!(update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap());
    let ready = read_manifest(&temp.outbox()).unwrap();
    let ready_bytes = fs::read(temp.outbox().join("manifest.json")).unwrap();
    // 图片就绪不会产生新的修订
    assert_eq!(
        ready.revision, initial.revision,
        "asset completion does not invent a new snapshot"
    );
    // 发件箱里的图片与原始内容一致
    assert_eq!(
        fs::read(temp.outbox().join("assets").join(&descriptor.hash)).unwrap(),
        bytes
    );
    // 清空时图片一并删除
    update(&temp.outbox(), &temp.0, None, None, None).unwrap();
    assert!(!temp.outbox().join("assets").join(&descriptor.hash).exists());
    // 若设置了互操作目录，则把清单、图片与清空清单导出，供 Swift 测试读取
    if let Some(directory) = std::env::var_os("LOKI_WATCH_INTEROP") {
        let directory = std::path::PathBuf::from(directory);
        assert!(
            directory.is_absolute(),
            "interop output must be an absolute test path"
        );
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("manifest.json"), ready_bytes).unwrap();
        // 写入资源文件
        fs::write(directory.join(&descriptor.hash), bytes).unwrap();
        // 复制清单为清除指令
        fs::copy(
            // 源：外发清单
            temp.outbox().join("manifest.json"),
            // 目标：clear.json
            directory.join("clear.json"),
        )
        // 复制成功
        .unwrap();
    }
}

#[test]
// 非法输入不会替换有效快照，也不会覆盖已损坏的状态
fn invalid_input_does_not_replace_a_valid_snapshot_or_overwrite_bad_state() {
    let temp = Temp::new();
    let raw = snapshot();
    // 先写入一份有效快照
    update(&temp.outbox(), &temp.0, Some(SOURCE), Some(&raw), Some(7)).unwrap();
    let first = fs::read(temp.outbox().join("manifest.json")).unwrap();
    // 来源非法的更新被拒绝
    assert!(
        update(
            &temp.outbox(),
            &temp.0,
            Some("not-a-source"),
            Some(&raw),
            Some(7)
        )
        // 应被拒绝
        .is_err()
    );
    // 原清单一字节未变
    assert_eq!(
        fs::read(temp.outbox().join("manifest.json")).unwrap(),
        first
    );
    // 清单已损坏：更新失败，且不覆盖损坏的文件
    fs::write(temp.outbox().join("manifest.json"), b"broken").unwrap();
    assert!(update(&temp.outbox(), &temp.0, None, None, None).is_err());
    assert_eq!(
        fs::read(temp.outbox().join("manifest.json")).unwrap(),
        b"broken"
    );
}

#[test]
// 路径被替换与修订号溢出时失败关闭
fn path_substitution_and_revision_overflow_fail_closed() {
    let temp = Temp::new();
    // 先建立空状态
    update(&temp.outbox(), &temp.0, None, None, None).unwrap();
    // 把修订号改为 u64 最大值
    let mut manifest = read_manifest(&temp.outbox()).unwrap();
    manifest.revision = u64::MAX.to_string();
    fs::write(
        temp.outbox().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    // 再更新会溢出，必须报错
    assert!(
        update(
            &temp.outbox(),
            &temp.0,
            Some(SOURCE),
            Some(&snapshot()),
            Some(7)
        )
        // 应被拒绝
        .is_err()
    );
    // Unix：清单被替换成符号链接
    #[cfg(unix)]
    {
        // 链接指向的“受害”文件
        let victim = temp.0.join("victim.json");
        fs::write(&victim, b"untouched").unwrap();
        fs::remove_file(temp.outbox().join("manifest.json")).unwrap();
        std::os::unix::fs::symlink(&victim, temp.outbox().join("manifest.json")).unwrap();
        // 更新失败
        assert!(update(&temp.outbox(), &temp.0, None, None, None).is_err());
        // 受害文件没被写入
        assert_eq!(fs::read(victim).unwrap(), b"untouched");
    }
}
