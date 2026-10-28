// 引入 Foundation
import Foundation

/// 在总额下方的小号精确行中,保留原始的十进制数字。
// 保留原始的十进制数字，作为总量下方小而精确的一行：每三位加逗号
func amountText(_ amount: TokenAmount) -> String {
    // 金额文本
    let text = amountLabel(amount)
    // 非纯数字原样返回
    guard !text.isEmpty, text.utf8.allSatisfy({ (48...57).contains($0) }) else { return text }
    // 从低位起每三位一组
    let reversed = Array(text.reversed())
    // 每三位一组
    return stride(from: 0, to: reversed.count, by: 3)
        // 反转后用逗号连接
        .map { String(reversed[$0..<min($0 + 3, reversed.count)].reversed()) }
        // 反转并用逗号连接
        .reversed().joined(separator: ",")
}

/// 原版手表总额使用 K/M/B 单位,保留两位小数并做整数舍入。
// 手表上的总量使用 K/M/B，保留两位小数并做整数四舍五入
func compactAmountText(_ amount: TokenAmount) -> String {
    // 金额文本
    let text = amountLabel(amount)
    // 非数字或不足一千原样返回
    guard !text.isEmpty, text.utf8.allSatisfy({ (48...57).contains($0) }),
          // 须是不小于 1000 的整数
          let total = UInt64(text), total >= 1_000 else { return text }
    // 各档位及后缀
    let scales: [UInt64] = [1_000, 1_000_000, 1_000_000_000]
    // 单位后缀
    let suffixes = ["K", "M", "B"]
    // 取不超过总量的最大档位
    var unit = scales.lastIndex(where: { total >= $0 }) ?? 0
    // 四舍五入可能进位，所以循环
    while true {
        // 当前单位的倍数
        let scale = scales[unit]
        // 整数部分
        var whole = total / scale
        // 用较小的刻度做除法,而不是把 UInt64 的余数乘以 100。
        // 小数部分：先缩小除数，避免把 UInt64 余数乘以 100 而溢出
        var fraction = ((total % scale) + scale / 200) / (scale / 100)
        // 进位
        if fraction == 100 { whole += 1; fraction = 0 }
        // 整数部分达到 1000 则升一档
        if whole >= 1_000 && unit < scales.count - 1 { unit += 1; continue }
        // 没有小数时不带小数点
        guard fraction != 0 else { return "\(whole)\(suffixes[unit])" }
        // 去掉末尾多余的 0
        let decimals = fraction % 10 == 0 ? String(fraction / 10)
            // 小数部分补零
            : (fraction < 10 ? "0\(fraction)" : String(fraction))
        // 整数加小数加单位
        return "\(whole).\(decimals)\(suffixes[unit])"
    }
}

// 简短的质量说明：完整度与置信度
func amountQuality(_ amount: TokenAmount) -> String {
    // 部分、范围未知
    let completeness = ["partial": "部分", "unknown": "范围未知"][amount.completeness]
    // 估算、待核对
    let confidence = ["derived": "估算", "suspected": "待核对"][amount.confidence]
    // 完整性与置信度用逗号连接
    return [completeness, confidence].compactMap { $0 }.joined(separator: "·")
}

// 用量文本：可用显示数值，其余显示状态
private func amountLabel(_ amount: TokenAmount) -> String {
    // 按可用性取文本
    switch amount.availability {
    // 可用时取总量
    case "available": return amount.total ?? "不可用"
    // 无记录
    case "noRecords": return "无记录"
    // 其余为不可用
    default: return "不可用"
    }
}
