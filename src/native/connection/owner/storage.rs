//! 将当前连接的配对簿和可信快照交给持久化层。
use super::Owner;
use crate::native::connection::{
    MAX_FRAME, Material, err,
    persistence::{atomic_write, erase_snapshot, write_records},
    snapshot_admission::validate_snapshot,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};

impl Owner {
    // 保存配对簿到系统安全存储
    pub(super) fn save_records(&self, records: &BTreeMap<String, Material>) -> Result<(), String> {
        write_records(&self.credential_service, &self.id, records)
    }

    // 把最近的快照连同来源与接收时间写入缓存文件，失败忽略
    pub(super) fn write_snapshot(&self) {
        // 估算快照缓存的大小
        let _ = serde_json::to_vec(&json!({"sourceId":self.selected,"receivedAtMs":self.view["receivedAtMs"],"snapshot":self.view["snapshot"]}))
            .map_err(err)
            .and_then(|bytes| atomic_write(&self.root.join("snapshot.json"), &bytes));
    }

    /// 只恢复仍受信任来源的合法快照；它带原接收时间，不能显示为实时。
    // 只恢复仍受信任来源的合法快照；它带原接收时间，不能显示为实时
    pub(super) fn restore_cache(&mut self) {
        // 不保留快照的宿主清掉旧缓存
        if !self.keep_snapshot {
            erase_snapshot(&self.root);
            return;
        }
        // 读取缓存，大小不得超过单帧上限
        let Some(v) = fs::read(self.root.join("snapshot.json"))
            .ok()
            .filter(|b| b.len() <= MAX_FRAME)
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        // 否则忽略
        else {
            return;
        };
        // 来源必须仍有配对记录
        let Some(id) = v["sourceId"]
            .as_str()
            .filter(|id| self.records.contains_key(*id))
        // 否则忽略
        else {
            return;
        };
        // 快照必须通过准入校验且有接收时间
        if validate_snapshot(&v["snapshot"]).is_err() || v["receivedAtMs"].as_u64().is_none() {
            return;
        }
        // 恢复选中的桌面与展示内容
        self.selected = Some(id.to_owned());
        self.view["selectedId"] = json!(id);
        self.view["snapshot"] = v["snapshot"].clone();
        self.view["receivedAtMs"] = v["receivedAtMs"].clone();
        self.view["cacheSourceId"] = json!(id);
        // 状态：显示最近缓存，等待桌面连接
        self.view["status"] = json!("显示最近缓存，等待桌面连接…");
        // 同时恢复本地已校验的图片
        self.restore_cached_images();
    }
}
