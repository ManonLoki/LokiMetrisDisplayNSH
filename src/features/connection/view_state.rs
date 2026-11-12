//! 连接事实转换为公开展示状态；缓存数据不能让会话自行变为在线。
use super::{
    // 设备标识与名称的校验函数
    identity::{name_error, valid_device_id},
    snapshot::project_snapshot,
};
use crate::shared::contracts::snapshot::safe_unsigned;
use serde_json::{Value, json};

// 空视图：未选择、未连接、无快照
pub(crate) fn empty_view(device_id: &str, device_name: &str) -> Value {
    json!({
        // 设备标识与名称，尚无主机和选中项
        "deviceId": device_id, "deviceName": device_name, "hosts": [], "selectedId": null,
        // 状态、错误与配对码
        "status": "正在发现桌面端…", "error": null, "pairingCode": null,
        // 配对期限、连接状态与快照
        "pairingDeadlineMs": null, "connected": false, "snapshot": null,
        // 接收时间、实时标记、缓存来源与已配对
        "receivedAtMs": null, "live": false, "cacheSourceId": null, "paired": {},
    })
}

// 把原生端口的状态文本转换为公开视图，校验每个字段
pub(crate) fn native_view(raw: &str) -> Result<Value, String> {
    // 解析 JSON，失败给出统一错误
    let value: Value = serde_json::from_str(raw).map_err(|_| "原生连接返回了无效状态")?;
    // 必须是对象
    if !value.is_object() {
        return Err("原生连接返回了无效状态".into());
    }
    // 设备 ID 必须是规范格式
    let device_id = value["deviceId"].as_str().filter(|id| valid_device_id(id));
    // 设备名称必须合法
    let device_name = value["deviceName"]
        .as_str()
        .filter(|name| name_error(name).is_none());
    // 主机列表必须是数组
    let hosts = value["hosts"].as_array();
    // 三者缺一则视为无效设备信息
    let (Some(device_id), Some(device_name), Some(hosts)) = (device_id, device_name, hosts) else {
        return Err("原生连接返回了无效设备信息".into());
    };
    // 快照为空保持空，否则必须通过投影校验
    let snapshot = if value["snapshot"].is_null() {
        Value::Null
    // 否则分支
    } else {
        // 投影快照，未通过校验则报错
        project_snapshot(&value["snapshot"], &value["images"]).ok_or("桌面快照未通过格式校验")?
    };
    let mut public_hosts = Vec::with_capacity(hosts.len());
    let mut paired = json!({});
    for host in hosts {
        let id = host["id"].as_str().filter(|id| valid_device_id(id));
        // 字段缺失或类型不对则整体无效
        if !host.is_object()
            || id.is_none()
            // 或者名称不是字符串
            || !host["name"].is_string()
            // 或者已配对标记不是布尔
            || !host["paired"].is_boolean()
            // 或者可用标记不是布尔
            || !host["available"].is_boolean()
        {
            return Err("原生连接返回了无效桌面信息".into());
        }
        let id = id.unwrap();
        // 只保留四个公开字段
        public_hosts.push(json!({"id": id, "name": host["name"], "paired": host["paired"], "available": host["available"]}));
        // 已配对的主机补充保存时间
        if host["paired"] == true {
            // 保存时间，无效按 0
            let saved = safe_unsigned(&value["paired"][id]["savedAtMs"]).unwrap_or(0);
            paired[id] = json!({"clientId": id, "savedAtMs": saved});
        }
    }
    let received = safe_unsigned(&value["receivedAtMs"]);
    // 只有存在快照和接收时间才可能“已连接”，缓存不能让会话变为在线
    let connected = value["connected"] == true && !snapshot.is_null() && received.is_some();
    let selected = value["selectedId"].as_str();
    // 配对码必须是四位数字
    let code = value["pairingCode"]
        .as_str()
        // 配对码须为四位数字
        .filter(|code| code.len() == 4 && code.bytes().all(|byte| byte.is_ascii_digit()));
    let next = json!({
        "deviceId": device_id, "deviceName": device_name,
        // 主机、选中项与已配对
        "hosts": public_hosts, "selectedId": selected, "paired": paired,
        // 状态，缺省为“正在发现桌面端…”
        "status": value["status"].as_str().unwrap_or("正在发现桌面端…"),
        "error": value["error"].as_str(), "assetWarning": value["assetWarning"].as_str(),
        "pairingCode": code, "pairingDeadlineMs": value["pairingDeadlineMs"].as_number(),
        // 是否已连接；实时标记须同时已连接
        "connected": connected, "live": value["live"] == true && connected,
        // 没有快照时不带接收时间
        "receivedAtMs": if snapshot.is_null() { None } else { received },
        // 缓存来源：优先用快照声明的，其次用选中项
        "cacheSourceId": value["cacheSourceId"].as_str().or(if snapshot.is_null() { None } else { selected }),
        "snapshot": snapshot,
    });
    Ok(next)
}

#[cfg(test)]
#[path = "view_state_test.rs"]
mod view_state_test;
