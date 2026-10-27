use super::*;
use crate::{
    summary::{
        self,
        summary_test::{SOURCE, message},
    },
    widget_http,
};
use serde_json::json;

const DEVICE: &str = "d2d2d2d2-3333-4ddd-8ddd-dddddddddddd";
const OTHER: &str = "e3e3e3e3-4444-4eee-8eee-eeeeeeeeeeee";

fn directory(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("loki-store-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn record(source: &str, received: i64) -> Value {
    let mut message = message();
    message["sourceId"] = json!(source);
    summary::from_message(&message, source, received).unwrap()
}

fn target(source: &str) -> Value {
    widget_http::target(
        source,
        DEVICE,
        "192.168.1.20",
        51_303,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    )
    .unwrap()
}

#[test]
fn authenticated_writes_keep_generation_and_reject_older_results() {
    let dir = directory("auth");
    let store = Store::new(&dir);
    fs::write(dir.join("display-phone-v1.json"), "{}").unwrap();
    assert!(
        store
            .save_authenticated(&record(SOURCE, 2_000), Some(&target(SOURCE)))
            .unwrap()
    );
    assert!(
        !dir.join("display-phone-v1.json").exists(),
        "legacy native cache removed"
    );
    let first = store.load_target().unwrap().unwrap();
    // 同一连接重复授予不改变代次，在途 HTTP 不会被误判为已撤销。
    assert!(
        !store
            .save_authenticated(&record(SOURCE, 1_000), Some(&target(SOURCE)))
            .unwrap()
    );
    assert_eq!(
        store.load_target().unwrap().unwrap()["generation"],
        first["generation"]
    );
    assert_eq!(store.load().unwrap().unwrap()["receivedAtMs"], 2_000);
    assert!(store.renew(SOURCE, 30_000).unwrap());
    assert_eq!(
        store.load().unwrap().unwrap()["validUntilMs"],
        30_000 + summary::LEASE_MS
    );
    assert!(
        !store.renew(SOURCE, 1_000_000).unwrap(),
        "expired lease is not renewed"
    );
}

#[test]
fn fetched_results_respect_revocation_and_newer_app_data() {
    let dir = directory("fetch");
    let store = Store::new(&dir);
    store
        .save_authenticated(&record(SOURCE, 1_000), Some(&target(SOURCE)))
        .unwrap();
    let granted = store.load_target().unwrap().unwrap();
    assert!(
        store
            .save_fetched(&record(SOURCE, 5_000), &granted, 5_000)
            .unwrap()
    );
    // 请求开始后 App 写入了更新摘要：迟到的 HTTP 不覆盖。
    store
        .save_authenticated(&record(SOURCE, 9_000), None)
        .unwrap();
    assert!(
        store.load_target().unwrap().is_none(),
        "authenticated write without grant revokes it"
    );
    assert!(
        !store
            .save_fetched(&record(SOURCE, 8_000), &granted, 8_000)
            .unwrap()
    );
}

#[test]
fn select_and_forget_only_drop_matching_sources() {
    let dir = directory("select");
    let store = Store::new(&dir);
    store
        .save_authenticated(&record(SOURCE, 1_000), Some(&target(SOURCE)))
        .unwrap();
    store.select(SOURCE).unwrap();
    assert!(store.load().unwrap().is_some());
    store.forget(OTHER).unwrap();
    assert!(store.load().unwrap().is_some());
    store.select(OTHER).unwrap();
    assert!(store.load().unwrap().is_none() && store.load_target().unwrap().is_none());
    assert!(store.select("bad").is_err());
}

/// 平台密钥库加密后，磁盘上不出现读取密钥明文，读取仍得到原凭据。
#[test]
fn sealed_targets_never_store_the_read_key_in_plain_text() {
    // 密钥只允许在进程启动时配置一次；单独进程避免并行测试被中途切换到加密存储。
    const CHILD: &str = "LOKI_SEALED_STORE_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "store::store_test::sealed_targets_never_store_the_read_key_in_plain_text",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(
            status.success(),
            "isolated process must verify sealed target storage"
        );
        return;
    }
    struct Xor;
    impl Sealer for Xor {
        fn seal(&self, plain: &[u8]) -> Option<Vec<u8>> {
            Some(plain.iter().map(|b| b ^ 0x5a).collect())
        }
        fn open(&self, sealed: &[u8]) -> Option<Vec<u8>> {
            self.seal(sealed)
        }
    }
    set_sealer(Box::new(Xor));
    let dir = directory("sealed");
    let store = Store::new(&dir);
    let granted = target(SOURCE);
    store
        .save_authenticated(&record(SOURCE, 1_000), Some(&granted))
        .unwrap();
    let raw = fs::read_to_string(dir.join(TARGET)).unwrap();
    assert!(raw.contains("sealed") && !raw.contains(granted["key"].as_str().unwrap()));
    assert_eq!(store.load_target().unwrap().unwrap(), granted);
}
