// 引入被测存储
use super::*;
// 引入摘要夹具与 HTTP 协议
use crate::{
    summary::{
        self,
        summary_test::{SOURCE, message},
    },
    widget_http,
};
use serde_json::json;

// 测试用的设备 ID
const DEVICE: &str = "d2d2d2d2-3333-4ddd-8ddd-dddddddddddd";
// 另一个来源的 ID
const OTHER: &str = "e3e3e3e3-4444-4eee-8eee-eeeeeeeeeeee";

// 创建独立的临时目录
fn directory(name: &str) -> PathBuf {
    // 以名称加进程号命名，避免用例间互相干扰
    let path = std::env::temp_dir().join(format!("loki-store-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

// 构造一条摘要记录
fn record(source: &str, received: i64) -> Value {
    // 以测试消息为基础，换成指定来源
    let mut message = message();
    message["sourceId"] = json!(source);
    summary::from_message(&message, source, received).unwrap()
}

// 构造一份读取凭据
fn target(source: &str) -> Value {
    // 构造读取凭据（局域网地址、固定端口与测试密钥）
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
// 认证写入保留凭据代次，并拒绝更旧的结果
fn authenticated_writes_keep_generation_and_reject_older_results() {
    let dir = directory("auth");
    let store = Store::new(&dir);
    // 放一个旧格式文件，新格式写入后应被删除
    fs::write(dir.join("display-phone-v1.json"), "{}").unwrap();
    // 认证写入接受 2000ms 的摘要和凭据
    assert!(
        store
            .save_authenticated(&record(SOURCE, 2_000), Some(&target(SOURCE)))
            .unwrap()
    );
    assert!(
        !dir.join("display-phone-v1.json").exists(),
        "legacy native cache removed"
    );
    // 记下首次授予的凭据，用于比较代次
    let first = store.load_target().unwrap().unwrap();
    // 同一连接重复授予不改变代次，在途 HTTP 不会被误判为已撤销。
    // 同一连接重复授予但摘要更旧：不接受
    assert!(
        !store
            .save_authenticated(&record(SOURCE, 1_000), Some(&target(SOURCE)))
            .unwrap()
    );
    // 凭据的代次保持不变，在途 HTTP 不会被误判为已撤销
    assert_eq!(
        store.load_target().unwrap().unwrap()["generation"],
        first["generation"]
    );
    // 摘要仍是较新的那份
    assert_eq!(store.load().unwrap().unwrap()["receivedAtMs"], 2_000);
    // 租约内续租成功
    assert!(store.renew(SOURCE, 30_000).unwrap());
    // 续租后的截止时间 = 现在 + 租约
    assert_eq!(
        store.load().unwrap().unwrap()["validUntilMs"],
        30_000 + summary::LEASE_MS
    );
    // 已过期的租约不能续
    assert!(
        !store.renew(SOURCE, 1_000_000).unwrap(),
        "expired lease is not renewed"
    );
}

#[test]
// HTTP 取回的结果要遵守撤销，并让位于主应用更新的数据
fn fetched_results_respect_revocation_and_newer_app_data() {
    let dir = directory("fetch");
    let store = Store::new(&dir);
    // 先授予凭据并写入摘要
    store
        .save_authenticated(&record(SOURCE, 1_000), Some(&target(SOURCE)))
        .unwrap();
    let granted = store.load_target().unwrap().unwrap();
    // 凭据仍有效时，HTTP 取回的新摘要被接受
    assert!(
        store
            .save_fetched(&record(SOURCE, 5_000), &granted, 5_000)
            .unwrap()
    );
    // 请求开始后 App 写入了更新摘要：迟到的 HTTP 不覆盖。
    // 请求开始后 App 写入了更新的摘要（且不带凭据）：迟到的 HTTP 不应覆盖
    store
        .save_authenticated(&record(SOURCE, 9_000), None)
        .unwrap();
    // 不带凭据的认证写入同时撤销了凭据
    assert!(
        store.load_target().unwrap().is_none(),
        "authenticated write without grant revokes it"
    );
    // 凭据已被撤销，迟到的 HTTP 结果不被接受
    assert!(
        !store
            .save_fetched(&record(SOURCE, 8_000), &granted, 8_000)
            .unwrap()
    );
}

#[test]
// 选择与忘记只删除匹配的来源
fn select_and_forget_only_drop_matching_sources() {
    let dir = directory("select");
    let store = Store::new(&dir);
    store
        .save_authenticated(&record(SOURCE, 1_000), Some(&target(SOURCE)))
        .unwrap();
    // 选择当前来源：保留
    store.select(SOURCE).unwrap();
    assert!(store.load().unwrap().is_some());
    // 忘记另一个来源：不影响当前的
    store.forget(OTHER).unwrap();
    assert!(store.load().unwrap().is_some());
    // 选择另一个来源：当前来源的摘要和凭据被撤销
    store.select(OTHER).unwrap();
    assert!(store.load().unwrap().is_none() && store.load_target().unwrap().is_none());
    // 非法的来源 ID 报错
    assert!(store.select("bad").is_err());
}
