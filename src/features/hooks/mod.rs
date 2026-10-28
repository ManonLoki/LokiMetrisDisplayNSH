//! Hook 状态属于业务事实；页码、单元格和详情展开仍由 UI 控制。
// 引入 JSON 值与构造宏
use serde_json::{Value, json};

// 只处理 hooks.label：把行为状态映射为中文文案
pub(crate) fn dispatch(op: &str, args: &[Value]) -> Option<Result<Value, String>> {
    // 仅处理 hooks.label
    (op == "hooks.label").then(|| {
        // 按行为取标签
        Ok(json!(match args.first().and_then(Value::as_str) {
            // 空闲
            Some("idle") => "空闲",
            // 运行
            Some("running") => "运行",
            // 询问
            Some("asking") => "询问",
            // 其余按出错处理
            _ => "出错",
        }))
    })
}

// 测试模块
#[cfg(test)]
#[path = "hooks_test.rs"]
mod hooks_test;
