// 引入 SwiftUI
import SwiftUI

/// App、Watch 与小组件共用的套餐 Tag；长文字保留完整的辅助功能描述。
// 套餐徽标：App、Watch 与小组件共用；长文字缩小后尾部省略，完整套餐保留在读屏描述中
struct ProviderPlanBadge: View {
    // 标签
    let label: String
    // 紧凑样式用更小的字号和内边距
    var compact = false
    // 字号随动态字体缩放
    @ScaledMetric(relativeTo: .caption2) private var textScale = 1.0

    // 视图主体
    var body: some View {
        // 浅色半透明底、细描边的小标签
        Text(label)
            // 字号随紧凑模式变化
            .font(.system(size: compact ? 7 : 9 * textScale, weight: .medium))
            // 半透明白色
            .foregroundStyle(.white.opacity(0.86))
            // 单行并允许缩小，尾部截断
            .lineLimit(1).minimumScaleFactor(0.75).truncationMode(.tail)
            // 内边距
            .padding(.horizontal, compact ? 3 : 4).padding(.vertical, compact ? 1 : 2)
            // 圆角背景
            .background(.white.opacity(0.09), in: RoundedRectangle(cornerRadius: 4, style: .continuous))
            // 圆角描边
            .overlay { RoundedRectangle(cornerRadius: 4, style: .continuous).strokeBorder(.white.opacity(0.18), lineWidth: 0.5) }
            // 读屏文案：套餐
            .accessibilityLabel("套餐 \(label)")
    }
}

/// 名称与 Tag 始终同行；名称和额度数字优先，长套餐缩小后尾部省略。
/// 完整套餐始终保留在读屏描述中，小组件同样可读。
// 供应商标题：名称与徽标始终同行，名称和额度数字优先；完整套餐始终在读屏描述里，小组件同样可读
struct ProviderQuotaTitle: View {
    // 名称
    let name: String
    // 套餐
    let plan: String?
    // 字体
    var font: Font = .caption.weight(.semibold)
    // 是否紧凑
    var compact = false

    // 视图主体
    var body: some View {
        // 名称不换行且不被压缩，优先于徽标
        HStack(alignment: .bottom, spacing: compact ? 3 : 5) {
            // 名称：单行且不压缩
            Text(name).font(font).lineLimit(1).fixedSize(horizontal: true, vertical: false)
                // 优先显示
                .layoutPriority(1)
            // 有套餐才显示徽标
            if let plan { ProviderPlanBadge(label: plan, compact: compact) }
        }
        // 合并为一个读屏元素，朗读“名称，套餐 X”
        .accessibilityElement(children: .ignore)
        // 读屏文案：名称加套餐
        .accessibilityLabel(plan.map { "\(name)，套餐 \($0)" } ?? name)
    }
}
