//! 面向 NSH VM 的公开连接投影;配对机密绝不越过这条边界。
// 引入 owner
use super::owner::Owner;
// 引入 JSON
use serde_json::{Value, json};
// 引入有序集合
use std::collections::BTreeSet;

// 空视图：在业务层空视图基础上加图片映射
pub(super) fn empty_view(id: &str, name: &str) -> Value {
    // 空视图
    let mut view = crate::features::connection::view_state::empty_view(id, name);
    // 图片表为空
    view["images"] = json!({});
    // 返回视图
    view
}

// 所有者的方法
impl Owner {
    /// 发现与信任合并后的桌面列表：重复声明同一身份的广告不可选；已配对但未发现的保留忘记入口。
    // 发现与信任合并后的桌面列表：重复声明同一身份的广告不可选；已配对但未发现的保留忘记入口
    fn hosts(&self) -> Vec<Value> {
        // 已见标识集合
        let mut ids = BTreeSet::new();
        // 主机列表
        let mut hosts = Vec::new();
        // 先列出已发现的端点
        for e in self.endpoints.values() {
            // 同一身份出现多次视为不可信，跳过
            if self.endpoints.values().filter(|v| v.id == e.id).count() != 1 {
                // 跳过
                continue;
            }
            // 记录标识
            ids.insert(e.id.clone());
            // 加入主机条目
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
        // 返回主机列表
        hosts
    }

    // 生成公开投影文本
    pub(super) fn projection(&mut self) -> String {
        // 更新主机列表
        self.view["hosts"] = json!(self.hosts());
        // 已配对映射
        let mut paired = serde_json::Map::new();
        // 只暴露客户端 ID 与保存时间，不含秘密
        for (id, r) in &self.records {
            // 记录客户端与保存时间
            paired.insert(id.clone(), json!({"clientId":id,"savedAtMs":r.saved_at_ms}));
        }
        // 写入视图
        self.view["paired"] = Value::Object(paired);
        // 序列化为文本
        self.view.to_string()
    }
}
