import SwiftUI

extension DisplayWidgetView {
    /// 待机使用的小尺寸也列出全部有效账户，每行并排呈现实际窗口，不用 +N 隐藏账户。
    // 列出全部有效账户，每行并排呈现实际窗口，不用 +N 隐藏账户（待机用的小尺寸也一样）
    func quotaProviders(_ quota: DisplayView.Quota, view: DisplayView, compact: Bool = false) -> some View {
        // 日期格式样式
        let style = Date.FormatStyle.dateTime.locale(SyncTimeFormat.locale)
        // 额度时间戳：同一天只显示时分，否则带月日
        let stamp = Calendar.current.isDate(quota.observedDate, inSameDayAs: entry.date)
            // 同一天只显示时分
            ? quota.observedDate.formatted(style.hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
            // 否则显示月日
            : quota.observedDate.formatted(style.month(.twoDigits).day(.twoDigits).hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
        // 生成额度更新说明
        let descriptions = try? view.labels(quotaTime: quota.observedDate.formatted(SyncTimeFormat.full), quotaCompactTime: stamp)
        // 纵向排列，间距随紧凑模式而变
        return VStack(alignment: .leading, spacing: compact ? 2 : 4) {
            // 标题行：“剩余额度”与更新时间
            HStack(spacing: 3) {
                // 小标题“剩余额度”
                Text("剩余额度")
                // 弹性占位
                Spacer(minLength: 0)
                // 紧凑的额度更新说明
                Text(descriptions?.quotaCompactUpdate ?? "")
            }
            // 字号随紧凑模式变化，次要色
            .font(.system(size: compact ? 7.5 : 8)).foregroundStyle(quotaSecondary)
            // 单行并允许缩小
            .lineLimit(1).minimumScaleFactor(0.8)
            // 忽略子元素的读屏
            .accessibilityElement(children: .ignore)
            // 读屏文案：额度更新
            .accessibilityLabel(descriptions?.quotaUpdate ?? "")
            // 逐个账户一行：标题加各窗口
            ForEach(quota.providers, id: \.provider) { provider in
                // 横向排列，垂直居中
                HStack(alignment: .center, spacing: 4) {
                    // 供应商标题
                    ProviderQuotaTitle(name: provider.name, plan: provider.plan,
                                       // 字体与紧凑模式
                                       font: .system(size: 9, weight: .semibold), compact: true)
                        // 强调色与固定宽度
                        .foregroundStyle(quotaAccent).frame(width: 59, alignment: .leading)
                    // 横向排列
                    HStack(spacing: 5) {
                        // 逐个额度窗口
                        ForEach(provider.windows, id: \.kind) { window in quotaWindow(window, compact: compact) }
                    // 占满宽度并优先显示
                    }.frame(maxWidth: .infinity, alignment: .leading).layoutPriority(1)
                }
                // 合并为一个读屏元素
                .accessibilityElement(children: .ignore)
                // 读屏文案：供应商描述
                .accessibilityLabel(descriptions?.providerDescriptions[provider.provider] ?? provider.description)
            }
        }
    }

    // 额度相关颜色：全彩模式用强调色，其余用白色；vibrant 模式用不透明灰阶保留对比
    var quotaAccent: Color { renderingMode == .fullColor ? accent : .white }
    // 次要文字颜色
    var quotaSecondary: Color {
        // 鲜艳模式下用浅灰，否则半透明白
        renderingMode == .vibrant ? Color(white: 0.8) : .white.opacity(0.65)
    }
    // 进度条底色
    var quotaTrack: Color {
        // 鲜艳模式下用深灰，否则淡白
        renderingMode == .vibrant ? Color(white: 0.35) : .white.opacity(0.18)
    }
    /// 与共享模型阈值一致：剩余不超过 20% 为红、不超过 50% 为黄，其余为绿。
    // 与共享模型的阈值一致：剩余不超过 20% 为红、不超过 50% 为黄，其余为绿
    func quotaFill(_ window: DisplayView.Window) -> Color {
        // 非全彩模式一律用白色
        guard renderingMode == .fullColor else { return .white }
        // 按级别取色：红、黄、绿
        return window.level == 0 ? .red : window.level == 1 ? .yellow : .green
    }

    /// 静态三点细条属于内容层，待机移除背景后仍存在；单色模式用不透明灰阶保留对比。
    // 单个额度窗口：标签、百分比和一条静态细条；细条属于内容层，待机移除背景后仍存在
    func quotaWindow(_ window: DisplayView.Window, compact: Bool) -> some View {
        // 纵向排列
        VStack(alignment: .leading, spacing: 1) {
            // 横向排列
            HStack(spacing: 1) {
                // 窗口标签
                Text(window.label).font(.system(size: compact ? 7.5 : 8))
                // 弹性占位
                Spacer(minLength: 0)
                // 百分比文字
                Text(window.percent).font(.system(size: compact ? 8 : 9, weight: .medium)).monospacedDigit()
                    // 不压缩并优先显示
                    .fixedSize(horizontal: true, vertical: false).layoutPriority(1)
            }
            // 读取容器宽度用于画填充条
            GeometryReader { geometry in
                // 底槽和按剩余比例填充的胶囊
                ZStack(alignment: .leading) {
                    // 胶囊形底条
                    Capsule().fill(quotaTrack)
                    // 填充条
                    Capsule().fill(quotaFill(window))
                        // 宽度按剩余比例
                        .frame(width: geometry.size.width * CGFloat(window.remainingTenths) / 1000)
                }
            }
            // 条高 3
            .frame(height: 3)
            // 对读屏隐藏
            .accessibilityHidden(true)
        }
        // 占满宽度
        .frame(maxWidth: .infinity)
        // 前景色
        .foregroundStyle(renderingMode == .vibrant ? Color.white : .white.opacity(0.85))
        // 单行并允许缩小
        .lineLimit(1).minimumScaleFactor(0.8)
    }
}
