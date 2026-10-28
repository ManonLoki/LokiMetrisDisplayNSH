// 引入 JSON 值
use serde_json::Value;

/// 在整数舍入完成之前保留完整的 u64 十进制数。若在此转为浮点,
/// 会悄悄改变大额总数的最后几位数字。
// 紧凑写法，最多到 M 级别（保留两位小数）
pub fn token_compact(raw: &str) -> String {
    // 保留两位小数的紧凑写法
    token_compact_to(raw, 2)
}

// 按最大单位格式化，全程使用整数，避免浮点丢失末位
fn token_compact_to(raw: &str, maximum_unit: usize) -> String {
    // 非纯数字原样返回
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        // 原样返回
        return raw.to_owned();
    }
    // 超出 u64 同样原样返回
    let Ok(total) = raw.parse::<u64>() else {
        // 原样返回
        return raw.to_owned();
    };
    // 不足一千不缩写
    if total < 1_000 {
        // 原样返回
        return raw.to_owned();
    }
    // 各档位的除数
    let scales = [
        // 千
        1_000_u64,
        // 百万
        1_000_000,
        // 十亿
        1_000_000_000,
        // 万亿
        1_000_000_000_000,
        // 千万亿
        1_000_000_000_000_000,
        // 百亿亿
        1_000_000_000_000_000_000,
    ];
    // 对应后缀
    let suffixes = ["K", "M", "B", "T", "P", "E"];
    // 从最大档位向下找到不超过总量的档位
    let mut unit = maximum_unit;
    // 选取不超过总量的最大单位
    while unit > 0 && total < scales[unit] {
        // 单位下调
        unit -= 1;
    }
    // 四舍五入后可能进位，需要重试
    loop {
        // 当前单位的倍数
        let scale = scales[unit];
        // 整数部分
        let mut whole = total / scale;
        // 所有 scale 可被 200 整除；先缩小除数，E 级也不会因余数乘 100 而溢出。
        // 小数部分（百分位，已四舍五入）；先缩小除数以免 E 级溢出
        let mut fraction = ((total % scale) + scale / 200) / (scale / 100);
        // 进位
        if fraction == 100 {
            // 整数部分进位
            whole += 1;
            // 小数部分归零
            fraction = 0;
        }
        // 整数部分达到 1000 则升一档
        if whole >= 1_000 && unit < maximum_unit {
            // 单位上调
            unit += 1;
            // 重新计算
            continue;
        }
        // 无小数则不带小数点
        return if fraction == 0 {
            // 整数加单位后缀
            format!("{whole}{}", suffixes[unit])
        // 否则分支
        } else {
            // 去掉末尾多余的 0
            let decimals = format!("{fraction:02}");
            // 带小数的写法
            format!(
                // 整数部分与小数部分
                "{whole}.{}{}",
                // 去掉小数末尾的 0
                decimals.trim_end_matches('0'),
                // 单位后缀
                suffixes[unit]
            )
        };
    }
}

// 精确写法：每三位加逗号
pub fn token_exact(raw: &str) -> String {
    // 非纯数字原样返回
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        // 原样返回
        return raw.to_owned();
    }
    // 带千分位的字符串
    let mut grouped = String::with_capacity(raw.len() + raw.len() / 3);
    // 逐位输出，距末尾为 3 的倍数处加逗号
    for (index, ch) in raw.chars().enumerate() {
        // 每三位加一个逗号
        if index > 0 && (raw.len() - index).is_multiple_of(3) {
            // 加逗号
            grouped.push(',');
        }
        // 加字符
        grouped.push(ch);
    }
    // 返回结果
    grouped
}

// 用量的展示文本：可用显示数值，其余显示状态
pub fn amount_label(amount: &Value) -> String {
    // 按可用性取标签
    match amount["availability"].as_str() {
        // 可用时取总量，缺失显示“—”
        Some("available") => amount["total"].as_str().unwrap_or("—").to_owned(),
        // 无记录
        Some("noRecords") => "无记录".to_owned(),
        // 不可用
        _ => "不可用".to_owned(),
    }
}

// 是否有数值
pub fn has_numeric_value(amount: &Value) -> bool {
    // 可用且总量非空才是数值
    amount["availability"] == "available" && !amount["total"].is_null()
}

// 无障碍朗读文本：数值加完整度与置信度说明
pub fn amount_accessibility(amount: &Value) -> String {
    // 先写 Token 总量
    let mut parts = vec![format!("Token {}", amount_label(amount))];
    // 按完整性补充说明
    match amount["completeness"].as_str() {
        // 部分统计
        Some("partial") => parts.push("部分统计".to_owned()),
        // 完整性未知
        Some("unknown") => parts.push("完整性未知".to_owned()),
        // 其余不加说明
        _ => {}
    }
    // 按置信度补充说明
    match amount["confidence"].as_str() {
        // 推算值
        Some("derived") => parts.push("推算".to_owned()),
        // 疑似值
        Some("suspected") => parts.push("疑似".to_owned()),
        // 其余不加说明
        _ => {}
    }
    // 用逗号连接
    parts.join("，")
}

// 测试模块
#[cfg(test)]
#[path = "formatting_test.rs"]
mod formatting_test;
