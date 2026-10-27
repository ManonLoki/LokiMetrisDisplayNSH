import Foundation

@main
struct PresentationTests {
    static var passed = 0

    static func main() throws {
        for (raw, expected) in [
            ("0", "0"), ("999", "999"), ("1000", "1K"), ("1004", "1K"),
            ("1005", "1.01K"), ("1010", "1.01K"), ("1500", "1.5K"),
            ("999994", "999.99K"), ("999995", "1M"), ("999999", "1M"),
            ("1004999", "1M"), ("1005000", "1.01M"),
            ("221193571", "221.19M"), ("999994999", "999.99M"),
            ("999995000", "1B"), ("999999999999", "1000B"),
            ("1000000000000", "1000B"), (String(UInt64.max), "18446744073.71B")
        ] {
            let value = amount(raw)
            try value.validate()
            check(compactAmountText(value) == expected, "integer rounding for \(raw)")
        }
        check(amountText(amount(String(UInt64.max))) == "18,446,744,073,709,551,615",
              "UInt64.max exact decimal never passes through floating point")
        check(amountText(amount("0")) == "0", "numeric zero stays available")
        check(amountText(amount("1000")) == "1,000", "exact line groups thousands")
        for (availability, expected) in [("noRecords", "无记录"), ("unavailable", "不可用")] {
            let value = amount(nil, availability: availability)
            try value.validate()
            check(amountText(value) == expected && compactAmountText(value) == expected,
                  "\(availability) is not displayed as numeric zero")
        }
        check(amountQuality(amount("1")) == "", "complete exact values have no qualifier")
        check(amountQuality(amount("1", completeness: "partial", confidence: "derived")) == "部分·估算",
              "partial estimates retain both qualifiers")
        check(amountQuality(amount("1", completeness: "unknown", confidence: "suspected")) == "范围未知·待核对",
              "unknown range and suspected data retain both qualifiers")
        // Rust tests read the same fixture, so the two display tables cannot drift apart.
        let fixture = try JSONSerialization.jsonObject(
            with: Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))) as! [String: Any]
        for pair in fixture["agents"] as! [[String]] {
            check(agentTitle(pair[0]) == pair[1], "agent display name for \(pair[0])")
        }
        for pair in fixture["behaviors"] as! [[String]] {
            check(behaviorTitle(pair[0]) == pair[1], "status label for \(pair[0])")
        }
        print("Watch presentation: \(passed) checks passed")
    }

    private static func amount(_ total: String?, availability: String = "available",
                               completeness: String = "complete", confidence: String = "exact") -> TokenAmount {
        TokenAmount(total: total, availability: availability, completeness: completeness, confidence: confidence)
    }

    private static func check(_ value: @autoclosure () -> Bool, _ message: String) {
        precondition(value(), message)
        passed += 1
    }
}
