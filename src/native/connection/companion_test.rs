// 引入被测类型
// 引入命令、材料与测试夹具
use super::{
    Command, Material,
    connection_test::{CLIENT, files, owner_for_test, snapshot},
};
use serde_json::{Value, json};

#[test]
// 只有受信任的 owner 才能导出；忘记桌面会持久化一次清空修订
fn only_a_trusted_owner_can_export_and_forgetting_persists_a_clear_revision() {
    // 临时目录
    let files = files();
    let (mut owner, _, _) = owner_for_test(&files);
    // 手表发件箱的清单文件
    let path = files.root.join("companion-v1/manifest.json");
    // 读取并解析清单
    let read = || serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap();
    // 选中桌面并放入快照，但配对簿里没有它
    owner.selected = Some(CLIENT.into());
    owner.view["snapshot"] = snapshot(vec![]);
    owner.view["receivedAtMs"] = json!(123);
    owner.commit();
    // 不受信任的缓存不应到达手表
    assert!(
        read()["snapshot"].is_null(),
        "untrusted cache must not reach Watch"
    );
    // 加入配对记录，使其受信任
    owner.records.insert(
        CLIENT.into(),
        Material {
            client_id: CLIENT.into(),
            device_id: files.device.clone(),
            name: "test".into(),
            secret: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [4; 32]),
            saved_at_ms: 1,
        },
    );
    // 发布视图
    owner.publish();
    // 提交变更
    owner.commit();
    // 现在清单里应有来源与精确的 Token 总量，修订号为 2
    let current = read();
    assert_eq!(current["sourceId"], CLIENT);
    assert_eq!(
        current["snapshot"]["tokens"]["all"]["total"],
        "18446744073709551615"
    );
    assert_eq!(current["revision"], "2");
    // 忘记桌面
    owner
        .command(Command::Forget {
            client_id: CLIENT.into(),
        })
        .unwrap();
    owner.commit();
    // 清单被清空，修订号递增为 3
    let cleared = read();
    assert_eq!(cleared["revision"], "3");
    assert!(cleared["sourceId"].is_null() && cleared["snapshot"].is_null());
    // 重新打开 owner（冷启动）
    let (mut reopened, _, _) = owner_for_test(&files);
    reopened.commit();
    // 冷启动不能抹掉“已忘记”的墓碑
    assert_eq!(
        read(),
        cleared,
        "cold start must not undo the forget tombstone"
    );
}
