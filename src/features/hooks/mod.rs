//! Hook 状态属于业务事实；页码、单元格和详情展开仍由 UI 控制。
// 把行为状态映射为中文文案
pub(crate) fn label(behavior: &str) -> &'static str {
    match behavior {
        // 空闲
        "idle" => "空闲",
        // 运行
        "running" => "运行",
        // 询问
        "asking" => "询问",
        // 其余按出错处理
        _ => "出错",
    }
}

// 测试模块
#[cfg(test)]
#[path = "hooks_test.rs"]
mod hooks_test;
