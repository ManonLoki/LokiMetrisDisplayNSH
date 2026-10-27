use serde_json::Value;

/// Keep the full u64 decimal until integer rounding has finished. A floating
/// point conversion here would silently change the last digits of large totals.
pub fn token_compact(raw: &str) -> String {
    token_compact_to(raw, 2)
}

fn token_compact_to(raw: &str, maximum_unit: usize) -> String {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return raw.to_owned();
    }
    let Ok(total) = raw.parse::<u64>() else {
        return raw.to_owned();
    };
    if total < 1_000 {
        return raw.to_owned();
    }
    let scales = [
        1_000_u64,
        1_000_000,
        1_000_000_000,
        1_000_000_000_000,
        1_000_000_000_000_000,
        1_000_000_000_000_000_000,
    ];
    let suffixes = ["K", "M", "B", "T", "P", "E"];
    let mut unit = maximum_unit;
    while unit > 0 && total < scales[unit] {
        unit -= 1;
    }
    loop {
        let scale = scales[unit];
        let mut whole = total / scale;
        // 所有 scale 可被 200 整除；先缩小除数，E 级也不会因余数乘 100 而溢出。
        let mut fraction = ((total % scale) + scale / 200) / (scale / 100);
        if fraction == 100 {
            whole += 1;
            fraction = 0;
        }
        if whole >= 1_000 && unit < maximum_unit {
            unit += 1;
            continue;
        }
        return if fraction == 0 {
            format!("{whole}{}", suffixes[unit])
        } else {
            let decimals = format!("{fraction:02}");
            format!(
                "{whole}.{}{}",
                decimals.trim_end_matches('0'),
                suffixes[unit]
            )
        };
    }
}

pub fn token_exact(raw: &str) -> String {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return raw.to_owned();
    }
    let mut grouped = String::with_capacity(raw.len() + raw.len() / 3);
    for (index, ch) in raw.chars().enumerate() {
        if index > 0 && (raw.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    grouped
}

pub fn amount_label(amount: &Value) -> String {
    match amount["availability"].as_str() {
        Some("available") => amount["total"].as_str().unwrap_or("—").to_owned(),
        Some("noRecords") => "无记录".to_owned(),
        _ => "不可用".to_owned(),
    }
}

pub fn has_numeric_value(amount: &Value) -> bool {
    amount["availability"] == "available" && !amount["total"].is_null()
}

pub fn amount_accessibility(amount: &Value) -> String {
    let mut parts = vec![format!("Token {}", amount_label(amount))];
    match amount["completeness"].as_str() {
        Some("partial") => parts.push("部分统计".to_owned()),
        Some("unknown") => parts.push("完整性未知".to_owned()),
        _ => {}
    }
    match amount["confidence"].as_str() {
        Some("derived") => parts.push("推算".to_owned()),
        Some("suspected") => parts.push("疑似".to_owned()),
        _ => {}
    }
    parts.join("，")
}

#[cfg(test)]
#[path = "formatting_test.rs"]
mod formatting_test;
