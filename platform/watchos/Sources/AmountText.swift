import Foundation

/// Keep the original decimal digits for the small, exact line below the total.
func amountText(_ amount: TokenAmount) -> String {
    let text = amountLabel(amount)
    guard !text.isEmpty, text.utf8.allSatisfy({ (48...57).contains($0) }) else { return text }
    let reversed = Array(text.reversed())
    return stride(from: 0, to: reversed.count, by: 3)
        .map { String(reversed[$0..<min($0 + 3, reversed.count)].reversed()) }
        .reversed().joined(separator: ",")
}

/// The original Watch total uses K/M/B, with two decimal places and integer rounding.
func compactAmountText(_ amount: TokenAmount) -> String {
    let text = amountLabel(amount)
    guard !text.isEmpty, text.utf8.allSatisfy({ (48...57).contains($0) }),
          let total = UInt64(text), total >= 1_000 else { return text }
    let scales: [UInt64] = [1_000, 1_000_000, 1_000_000_000]
    let suffixes = ["K", "M", "B"]
    var unit = scales.lastIndex(where: { total >= $0 }) ?? 0
    while true {
        let scale = scales[unit]
        var whole = total / scale
        // Divide by the smaller scale instead of multiplying a UInt64 remainder by 100.
        var fraction = ((total % scale) + scale / 200) / (scale / 100)
        if fraction == 100 { whole += 1; fraction = 0 }
        if whole >= 1_000 && unit < scales.count - 1 { unit += 1; continue }
        guard fraction != 0 else { return "\(whole)\(suffixes[unit])" }
        let decimals = fraction % 10 == 0 ? String(fraction / 10)
            : (fraction < 10 ? "0\(fraction)" : String(fraction))
        return "\(whole).\(decimals)\(suffixes[unit])"
    }
}

func amountQuality(_ amount: TokenAmount) -> String {
    let completeness = ["partial": "部分", "unknown": "范围未知"][amount.completeness]
    let confidence = ["derived": "估算", "suspected": "待核对"][amount.confidence]
    return [completeness, confidence].compactMap { $0 }.joined(separator: "·")
}

private func amountLabel(_ amount: TokenAmount) -> String {
    switch amount.availability {
    case "available": return amount.total ?? "不可用"
    case "noRecords": return "无记录"
    default: return "不可用"
    }
}
