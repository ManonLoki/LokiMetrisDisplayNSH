import SwiftUI

/// 额度页只显示账户名称、独立套餐 Tag 与实际窗口余额。
struct WatchProviderQuotaContent: View {
    let quota: ProviderQuota
    @Environment(\.dynamicTypeSize) private var dynamicType

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(quota.providers, id: \.provider) { provider in
                VStack(alignment: .leading, spacing: 7) {
                    ProviderQuotaTitle(name: provider.watchName, plan: provider.watchPlan)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    let layout = dynamicType.isAccessibilitySize
                        ? AnyLayout(VStackLayout(alignment: .leading, spacing: 8))
                        : AnyLayout(HStackLayout(spacing: 8))
                    layout {
                        ForEach(provider.watchWindows, id: \.kind) { value in
                            quotaProgress(value, label: value.watchLabel)
                                .frame(maxWidth: .infinity)
                        }
                    }
                }
            }
        }
        .privacySensitive()
        .accessibilityIdentifier("watch-provider-quota")
    }

    /// 直接显示桌面给出的剩余比例；缺少的窗口不占位，不按Token用量推算。
    private func quotaProgress(_ value: QuotaWindow, label: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(alignment: .firstTextBaseline, spacing: 4) {
                Text(label).font(.caption2).lineLimit(1).minimumScaleFactor(0.75)
                Spacer(minLength: 0)
                Text(value.watchPercent).font(.caption2.monospacedDigit()).fontWeight(.semibold)
                    .lineLimit(1).minimumScaleFactor(0.75)
            }
            GeometryReader { geometry in
                let color: Color = value.watchRemaining <= 200 ? .red : value.watchRemaining <= 500 ? .yellow : .green
                Capsule().fill(color.opacity(0.2))
                    .overlay(alignment: .leading) {
                        Capsule().fill(color)
                            .frame(width: geometry.size.width * CGFloat(value.watchRemaining) / 1000)
                    }
            }
            .frame(height: 4)
            .accessibilityHidden(true)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(label)额度剩余，\(value.watchPercent)")
    }
}

// Adapt the current public cache contract to the original Watch presentation.
private extension QuotaProvider {
    var watchName: String {
        ["codex": "Codex", "claude": "Claude", "cursor": "Cursor", "grok": "Grok"][provider] ?? provider
    }
    var watchPlan: String? {
        guard let plan, let first = plan.first else { return nil }
        return first.uppercased() + String(plan.dropFirst())
    }
    var watchWindows: [QuotaWindow] { [monthly, weekly, fiveHour].compactMap { $0 } }
}

private extension QuotaWindow {
    var watchLabel: String { ["monthly": "每月", "weekly": "每周", "fiveHour": "5h"][kind] ?? kind }
    var watchRemaining: Int { 1000 - usedTenths }
    var watchPercent: String {
        let remaining = watchRemaining
        return remaining % 10 == 0 ? "\(remaining / 10)%" : "\(remaining / 10).\(remaining % 10)%"
    }
}
