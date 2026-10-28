// 引入 SwiftUI
import SwiftUI

/// 额度页只显示账户名称、独立套餐 Tag 与实际窗口余额。
// 额度页内容：只显示账户名称、独立的套餐标签与实际窗口的余额
struct WatchProviderQuotaContent: View {
    // 供应商额度
    let quota: ProviderQuota
    // 读取动态字体大小
    @Environment(\.dynamicTypeSize) private var dynamicType

    // 视图主体
    var body: some View {
        // 纵向排列
        VStack(alignment: .leading, spacing: 10) {
            // 逐个账户
            ForEach(quota.providers, id: \.provider) { provider in
                // 纵向排列，间距 7
                VStack(alignment: .leading, spacing: 7) {
                    // 名称与套餐
                    ProviderQuotaTitle(name: provider.watchName, plan: provider.watchPlan)
                        // 左对齐占满
                        .frame(maxWidth: .infinity, alignment: .leading)
                    // 辅助功能字号下窗口纵向排列，否则横向并排
                    let layout = dynamicType.isAccessibilitySize
                        // 字体较大时纵向排列
                        ? AnyLayout(VStackLayout(alignment: .leading, spacing: 8))
                        // 否则横向排列
                        : AnyLayout(HStackLayout(spacing: 8))
                    // 套用布局
                    layout {
                        // 每个窗口一条进度
                        ForEach(provider.watchWindows, id: \.kind) { value in
                            // 额度进度条
                            quotaProgress(value, label: value.watchLabel)
                                // 占满宽度
                                .frame(maxWidth: .infinity)
                        }
                    }
                }
            }
        }
        // 隐私敏感
        .privacySensitive()
        // 测试标识
        .accessibilityIdentifier("watch-provider-quota")
    }

    /// 直接显示桌面给出的剩余比例；缺少的窗口不占位，不按Token用量推算。
    // 直接显示桌面给出的剩余比例；缺少的窗口不占位，也不按 Token 用量推算
    private func quotaProgress(_ value: QuotaWindow, label: String) -> some View {
        // 纵向排列
        VStack(alignment: .leading, spacing: 3) {
            // 标签与百分比
            HStack(alignment: .firstTextBaseline, spacing: 4) {
                // 窗口标签
                Text(label).font(.caption2).lineLimit(1).minimumScaleFactor(0.75)
                // 弹性占位
                Spacer(minLength: 0)
                // 百分比文字，等宽数字
                Text(value.watchPercent).font(.caption2.monospacedDigit()).fontWeight(.semibold)
                    // 单行并允许缩小
                    .lineLimit(1).minimumScaleFactor(0.75)
            }
            // 进度条：剩余不超过 20% 红、不超过 50% 黄，其余绿
            GeometryReader { geometry in
                // 按剩余量取颜色
                let color: Color = value.watchRemaining <= 200 ? .red : value.watchRemaining <= 500 ? .yellow : .green
                // 底色
                Capsule().fill(color.opacity(0.2))
                    // 叠加填充条
                    .overlay(alignment: .leading) {
                        // 进度条填充
                        Capsule().fill(color)
                            // 宽度按剩余比例
                            .frame(width: geometry.size.width * CGFloat(value.watchRemaining) / 1000)
                    }
            }
            // 条高 4
            .frame(height: 4)
            // 对读屏隐藏
            .accessibilityHidden(true)
        }
        // 合并为一个读屏元素
        .accessibilityElement(children: .ignore)
        // 读屏文案：窗口名与剩余比例
        .accessibilityLabel("\(label)额度剩余，\(value.watchPercent)")
    }
}

// 把当前的公开缓存契约适配为原版手表的呈现方式。
// 把当前的公开缓存契约适配到手表原有的展示
private extension QuotaProvider {
    // 供应商显示名
    var watchName: String {
        // 供应商显示名表
        ["codex": "Codex", "claude": "Claude", "cursor": "Cursor", "grok": "Grok"][provider] ?? provider
    }
    // 套餐首字母大写
    var watchPlan: String? {
        // 套餐首字母
        guard let plan, let first = plan.first else { return nil }
        // 首字母大写
        return first.uppercased() + String(plan.dropFirst())
    }
    // 窗口顺序：每月、每周、五小时
    var watchWindows: [QuotaWindow] { [monthly, weekly, fiveHour].compactMap { $0 } }
}

// 额度窗口的扩展
private extension QuotaWindow {
    // 窗口标签
    var watchLabel: String { ["monthly": "每月", "weekly": "每周", "fiveHour": "5h"][kind] ?? kind }
    // 剩余千分比
    var watchRemaining: Int { 1000 - usedTenths }
    // 剩余百分比，整十时不带小数
    var watchPercent: String {
        // 剩余量
        let remaining = watchRemaining
        // 整十显示整数百分比，否则带一位小数
        return remaining % 10 == 0 ? "\(remaining / 10)%" : "\(remaining / 10).\(remaining % 10)%"
    }
}
