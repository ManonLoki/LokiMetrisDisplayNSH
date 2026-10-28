// 引入 Foundation
import Foundation

// 展示测试入口
@main
struct PresentationTests {
    // 已通过的检查数
    static var passed = 0

    // 测试入口
    static func main() throws {
        // 紧凑文本的整数四舍五入：边界、进位与 UInt64 最大值
        for (raw, expected) in [
            ("0", "0"), ("999", "999"), ("1000", "1K"), ("1004", "1K"),
            ("1005", "1.01K"), ("1010", "1.01K"), ("1500", "1.5K"),
            ("999994", "999.99K"), ("999995", "1M"), ("999999", "1M"),
            ("1004999", "1M"), ("1005000", "1.01M"),
            ("221193571", "221.19M"), ("999994999", "999.99M"),
            ("999995000", "1B"), ("999999999999", "1000B"),
            ("1000000000000", "1000B"), (String(UInt64.max), "18446744073.71B")
        // 逐项检查
        ] {
            // 构造金额
            let value = amount(raw)
            // 先通过校验
            try value.validate()
            // 紧凑文本符合预期
            check(compactAmountText(value) == expected, "integer rounding for \(raw)")
        }
        // UInt64 最大值的精确十进制不经过浮点数
        check(amountText(amount(String(UInt64.max))) == "18,446,744,073,709,551,615",
              "UInt64.max exact decimal never passes through floating point")
        // 数值 0 仍是“可用”
        check(amountText(amount("0")) == "0", "numeric zero stays available")
        // 精确行每三位加逗号
        check(amountText(amount("1000")) == "1,000", "exact line groups thousands")
        // 无记录/不可用不显示为数字 0
        for (availability, expected) in [("noRecords", "无记录"), ("unavailable", "不可用")] {
            let value = amount(nil, availability: availability)
            try value.validate()
            check(amountText(value) == expected && compactAmountText(value) == expected,
                  "\(availability) is not displayed as numeric zero")
        }
        // 完整且精确的值没有限定词
        check(amountQuality(amount("1")) == "", "complete exact values have no qualifier")
        // 部分且估算：两个限定词都保留
        check(amountQuality(amount("1", completeness: "partial", confidence: "derived")) == "部分·估算",
              "partial estimates retain both qualifiers")
        // 范围未知且待核对：两个限定词都保留
        check(amountQuality(amount("1", completeness: "unknown", confidence: "suspected")) == "范围未知·待核对",
              "unknown range and suspected data retain both qualifiers")
        // Rust 测试读取同一份夹具,因此两张显示表不会各自漂移。
        // Rust 测试读取同一份夹具，两张显示名表不会各走各的
        let fixture = try JSONSerialization.jsonObject(
            with: Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))) as! [String: Any]
        // 代理显示名
        for pair in fixture["agents"] as! [[String]] {
            check(agentTitle(pair[0]) == pair[1], "agent display name for \(pair[0])")
        }
        // 状态文案
        for pair in fixture["behaviors"] as! [[String]] {
            check(behaviorTitle(pair[0]) == pair[1], "status label for \(pair[0])")
        }
        print("Watch presentation: \(passed) checks passed")
    }

    // 构造测试用的用量
    private static func amount(_ total: String?, availability: String = "available",
                               completeness: String = "complete", confidence: String = "exact") -> TokenAmount {
        TokenAmount(total: total, availability: availability, completeness: completeness, confidence: confidence)
    }

    // 断言并累计通过数
    private static func check(_ value: @autoclosure () -> Bool, _ message: String) {
        precondition(value(), message)
        passed += 1
    }
}
