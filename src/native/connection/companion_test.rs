use super::{
    Command, Material,
    connection_test::{CLIENT, files, owner_for_test, snapshot},
};
use serde_json::{Value, json};

#[test]
fn only_a_trusted_owner_can_export_and_forgetting_persists_a_clear_revision() {
    let files = files();
    let (mut owner, _, _) = owner_for_test(&files);
    let path = files.root.join("companion-v1/manifest.json");
    let read = || serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap();
    owner.selected = Some(CLIENT.into());
    owner.view["snapshot"] = snapshot(vec![]);
    owner.view["receivedAtMs"] = json!(123);
    owner.commit();
    assert!(
        read()["snapshot"].is_null(),
        "untrusted cache must not reach Watch"
    );
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
    owner.publish();
    owner.commit();
    let current = read();
    assert_eq!(current["sourceId"], CLIENT);
    assert_eq!(
        current["snapshot"]["tokens"]["all"]["total"],
        "18446744073709551615"
    );
    assert_eq!(current["revision"], "2");
    owner
        .command(Command::Forget {
            client_id: CLIENT.into(),
        })
        .unwrap();
    owner.commit();
    let cleared = read();
    assert_eq!(cleared["revision"], "3");
    assert!(cleared["sourceId"].is_null() && cleared["snapshot"].is_null());
    let (mut reopened, _, _) = owner_for_test(&files);
    reopened.commit();
    assert_eq!(
        read(),
        cleared,
        "cold start must not undo the forget tombstone"
    );
}
