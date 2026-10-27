//! 连接事实转换为公开展示状态；缓存数据不能让会话自行变为在线。
use super::{
    identity::{name_error, valid_device_id},
    snapshot::project_snapshot,
};
use crate::shared::contracts::{panel::panel_from_value, snapshot::safe_unsigned};
use serde_json::{Value, json};

pub(crate) fn empty_view(device_id: &str, device_name: &str) -> Value {
    json!({
        "deviceId": device_id, "deviceName": device_name, "hosts": [], "selectedId": null,
        "status": "正在发现桌面端…", "error": null, "pairingCode": null,
        "pairingDeadlineMs": null, "connected": false, "snapshot": null,
        "receivedAtMs": null, "live": false, "cacheSourceId": null, "paired": {},
    })
}

pub(crate) fn native_view(raw: &str, current: &Value) -> Result<Value, String> {
    let value: Value = serde_json::from_str(raw).map_err(|_| "原生连接返回了无效状态")?;
    if !value.is_object() {
        return Err("原生连接返回了无效状态".into());
    }
    let device_id = value["deviceId"].as_str().filter(|id| valid_device_id(id));
    let device_name = value["deviceName"]
        .as_str()
        .filter(|name| name_error(name).is_none());
    let hosts = value["hosts"].as_array();
    let (Some(device_id), Some(device_name), Some(hosts)) = (device_id, device_name, hosts) else {
        return Err("原生连接返回了无效设备信息".into());
    };
    let snapshot = if value["snapshot"].is_null() {
        Value::Null
    } else {
        project_snapshot(&value["snapshot"], &value["images"]).ok_or("桌面快照未通过格式校验")?
    };
    let mut public_hosts = Vec::with_capacity(hosts.len());
    let mut paired = json!({});
    for host in hosts {
        let id = host["id"].as_str().filter(|id| valid_device_id(id));
        if !host.is_object()
            || id.is_none()
            || !host["name"].is_string()
            || !host["paired"].is_boolean()
            || !host["available"].is_boolean()
        {
            return Err("原生连接返回了无效桌面信息".into());
        }
        let id = id.unwrap();
        public_hosts.push(json!({"id": id, "name": host["name"], "paired": host["paired"], "available": host["available"]}));
        if host["paired"] == true {
            paired[id] = json!({"clientId": id, "savedAtMs": 0});
        }
    }
    let received = safe_unsigned(&value["receivedAtMs"]);
    let connected = value["connected"] == true && !snapshot.is_null() && received.is_some();
    let selected = value["selectedId"].as_str();
    let code = value["pairingCode"]
        .as_str()
        .filter(|code| code.len() == 4 && code.bytes().all(|byte| byte.is_ascii_digit()));
    let _ = current;
    let mut next = serde_json::Map::new();
    let fields = json!({
        "deviceId": device_id, "deviceName": device_name,
        "hosts": public_hosts, "selectedId": selected, "paired": paired,
        "status": value["status"].as_str().unwrap_or("正在发现桌面端…"),
        "error": value["error"].as_str(), "assetWarning": value["assetWarning"].as_str(),
        "pairingCode": code, "pairingDeadlineMs": value["pairingDeadlineMs"].as_number(),
        "connected": connected, "live": value["live"] == true && connected,
        "receivedAtMs": if snapshot.is_null() { None } else { received },
        "cacheSourceId": value["cacheSourceId"].as_str().or(if snapshot.is_null() { None } else { selected }),
        "snapshot": snapshot,
    });
    next.extend(fields.as_object().unwrap().clone());
    next.remove("panelConfig");
    if value["panelConfig"].is_object() || value["panelConfig"].is_array() {
        next.insert(
            "panelConfig".into(),
            panel_from_value(&value["panelConfig"]),
        );
    }
    Ok(Value::Object(next))
}

#[cfg(test)]
#[path = "model_test.rs"]
mod model_test;
