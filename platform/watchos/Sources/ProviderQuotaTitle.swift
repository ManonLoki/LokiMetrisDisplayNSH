import SwiftUI

/// App、Watch 与小组件共用的套餐 Tag；长文字保留完整的辅助功能描述。
struct ProviderPlanBadge: View {
    let label: String
    var compact = false
    @ScaledMetric(relativeTo: .caption2) private var textScale = 1.0

    var body: some View {
        Text(label)
            .font(.system(size: compact ? 7 : 9 * textScale, weight: .medium))
            .foregroundStyle(.white.opacity(0.86))
            .lineLimit(1).minimumScaleFactor(0.75).truncationMode(.tail)
            .padding(.horizontal, compact ? 3 : 4).padding(.vertical, compact ? 1 : 2)
            .background(.white.opacity(0.09), in: RoundedRectangle(cornerRadius: 4, style: .continuous))
            .overlay { RoundedRectangle(cornerRadius: 4, style: .continuous).strokeBorder(.white.opacity(0.18), lineWidth: 0.5) }
            .accessibilityLabel("套餐 \(label)")
    }
}

/// 名称与 Tag 始终同行；名称和额度数字优先，长套餐缩小后尾部省略。
/// 完整套餐始终保留在读屏描述中，小组件同样可读。
struct ProviderQuotaTitle: View {
    let name: String
    let plan: String?
    var font: Font = .caption.weight(.semibold)
    var compact = false

    var body: some View {
        HStack(alignment: .bottom, spacing: compact ? 3 : 5) {
            Text(name).font(font).lineLimit(1).fixedSize(horizontal: true, vertical: false)
                .layoutPriority(1)
            if let plan { ProviderPlanBadge(label: plan, compact: compact) }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(plan.map { "\(name)，套餐 \($0)" } ?? name)
    }
}
