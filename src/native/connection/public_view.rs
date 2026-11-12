//! 面向 NSH VM 的公开连接投影;配对机密绝不越过这条边界。
use super::owner::Owner;
use serde_json::{Value, json};
use std::collections::BTreeSet;

// 空视图：在业务层空视图基础上加图片映射
pub(super) fn empty_view(id: &str, name: &str) -> Value {
    let mut view = crate::features::connection::view_state::empty_view(id, name);
    view["images"] = json!({});
    view
}

impl Owner {
    /// 发现与信任合并后的桌面列表：重复声明同一身份的广告不可选；已配对但未发现的保留忘记入口。
    // 发现与信任合并后的桌面列表：重复声明同一身份的广告不可选；已配对但未发现的保留忘记入口
    fn hosts(&self) -> Vec<Value> {
        let mut ids = BTreeSet::new();
        let mut hosts = Vec::new();
        // 先列出已发现的端点
        for e in self.endpoints.values() {
            // 同一身份出现多次视为不可信，跳过
            if self.endpoints.values().filter(|v| v.id == e.id).count() != 1 {
                continue;
            }
            ids.insert(e.id.clone());
            hosts.push(json!({"id":e.id,"name":e.name,"paired":self.records.contains_key(&e.id),"available":true}));
        }
        // 再补上已配对但当前未发现的桌面
        for id in self.records.keys() {
            // 已配对但当前未发现的桌面
            if !ids.contains(id) {
                // 以缩写标识显示为已配对桌面
                hosts.push(json!({"id":id,"name":format!("已配对桌面 · {}",&id[..8]),"paired":true,"available":false}));
            }
        }
        hosts
    }

    pub(super) fn projection(&mut self) -> String {
        self.view["hosts"] = json!(self.hosts());
        let mut paired = serde_json::Map::new();
        // 只暴露客户端 ID 与保存时间，不含秘密
        for (id, r) in &self.records {
            // 记录客户端与保存时间
            paired.insert(id.clone(), json!({"clientId":id,"savedAtMs":r.saved_at_ms}));
        }
        self.view["paired"] = Value::Object(paired);
        self.view.to_string()
    }
}
