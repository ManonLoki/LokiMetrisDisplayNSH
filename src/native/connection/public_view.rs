//! Public connection projection for the NSH VM; pairing secrets never cross this boundary.
use super::owner::Owner;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) fn empty_view(id: &str, name: &str) -> Value {
    let mut view = crate::features::connection::view_state::empty_view(id, name);
    view["images"] = json!({});
    view
}

impl Owner {
    /// 发现与信任合并后的桌面列表：重复声明同一身份的广告不可选；已配对但未发现的保留忘记入口。
    fn hosts(&self) -> Vec<Value> {
        let mut ids = BTreeSet::new();
        let mut hosts = Vec::new();
        for e in self.endpoints.values() {
            if self.endpoints.values().filter(|v| v.id == e.id).count() != 1 {
                continue;
            }
            ids.insert(e.id.clone());
            hosts.push(json!({"id":e.id,"name":e.name,"paired":self.records.contains_key(&e.id),"available":true}));
        }
        for id in self.records.keys() {
            if !ids.contains(id) {
                hosts.push(json!({"id":id,"name":format!("已配对桌面 · {}",&id[..8]),"paired":true,"available":false}));
            }
        }
        hosts
    }

    pub(super) fn projection(&mut self) -> String {
        self.view["hosts"] = json!(self.hosts());
        let mut paired = serde_json::Map::new();
        for (id, r) in &self.records {
            paired.insert(id.clone(), json!({"clientId":id,"savedAtMs":r.saved_at_ms}));
        }
        self.view["paired"] = Value::Object(paired);
        self.view.to_string()
    }
}
