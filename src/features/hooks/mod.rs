//! 提供 Hook 状态文案和分页规则，当前页由界面维护。
// 把行为状态映射为中文文案
pub(crate) fn label(behavior: &str) -> &'static str {
    match behavior {
        "idle" => "空闲",
        "running" => "运行",
        "asking" => "询问",
        _ => "出错",
    }
}

// 每页的格子数：1x1 一格，其余（2x2）四格
pub(crate) fn page_size(layout: &str) -> usize {
    if layout == "1x1" { 1 } else { 4 }
}

// 把 count 个 Hook 按布局排成页，每格是 Hook 的序号，空位为 None；没有 Hook 时仍有一页空格
pub(crate) fn pages(count: usize, layout: &str) -> Vec<Vec<Option<u32>>> {
    let size = page_size(layout);
    (0..count.max(1).div_ceil(size))
        .map(|page| {
            (0..size)
                .map(|cell| {
                    let index = page * size + cell;
                    (index < count).then_some(index as u32)
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
#[path = "hooks_test.rs"]
mod hooks_test;
