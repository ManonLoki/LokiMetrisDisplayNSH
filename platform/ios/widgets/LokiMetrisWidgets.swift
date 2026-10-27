import SwiftUI
import WidgetKit

/// 五种尺寸共用系统Timeline，系统唤醒时使用独立只读权限短时拉取摘要。
@main
struct LokiMetrisWidgets: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: WidgetContainer.kind, provider: DisplayTimeline()) { entry in
            DisplayWidgetView(entry: entry)
                .containerBackground(for: .widget) { WidgetCyberMaterial() }
                .privacySensitive()
        }
        .configurationDisplayName("app_display_name")
        .description("上次同步的 Token 用量、剩余额度与 Hooks 摘要。")
        .supportedFamilies([.systemSmall, .systemMedium, .accessoryInline, .accessoryCircular, .accessoryRectangular])
    }
}

/// Timeline 条目；snapshot 为空时显示未同步占位。
struct DisplayEntry: TimelineEntry {
    let date: Date
    let snapshot: WidgetContainer.Entry?
}

/// 本地缓存优先；Timeline唤醒时独立HTTP读取，系统决定实际执行时间。
struct DisplayTimeline: TimelineProvider {
    /// 系统占位：不读磁盘，直接显示未同步状态。
    func placeholder(in context: Context) -> DisplayEntry { DisplayEntry(date: Date(), snapshot: nil) }
    /// 小组件库预览与快速展示只读本地缓存，不发网络请求。
    func getSnapshot(in context: Context, completion: @escaping (DisplayEntry) -> Void) { completion(load()) }
    /// 有凭据时先 HTTP 拉取并落盘，再按 `WidgetContainer.refreshInterval` 申请下次刷新。
    /// 摘要仍在 45 秒租约内时跳过 HTTP；App 心跳可续租，HTTP 刚拉取的摘要也有初始租约。
    /// 这只用于抑制重复请求，不在组件界面中作为桌面实时在线的证明。
    func getTimeline(in context: Context, completion: @escaping (Timeline<DisplayEntry>) -> Void) {
        Task {
            // 租约内的摘要由 Rust 判断并跳过请求。
            if !context.isPreview { _ = await WidgetContainer.refresh(origin: "timeline") }
            let entry = load()
            completion(Timeline(entries: [entry], policy: .after(Date(timeIntervalSinceNow: WidgetContainer.refreshInterval))))
        }
    }
    /// 读取最近一次合法摘要；过期仍显示，并标注真实同步时间。
    private func load() -> DisplayEntry {
        let now = Date()
        return DisplayEntry(date: now, snapshot: WidgetContainer.load(atMs: now.epochMs))
    }
}

/// 每种组件各自组织信息；锁屏保持系统单色，桌面使用高对比数字。
struct DisplayWidgetView: View {
    @Environment(\.widgetFamily) private var family
    @Environment(\.widgetRenderingMode) private var renderingMode
    private let accent = Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255)
    private let violet = Color(red: 174 / 255, green: 140 / 255, blue: 255 / 255)
    let entry: DisplayEntry

    private var appDisplayName: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String ?? "LokiMetris"
    }

    @ViewBuilder var body: some View {
        if let record = entry.snapshot {
            switch family {
            case .accessoryInline:
                Text(labels(record)?.appleAccessoryInline ?? record.view.hooksText)
            case .accessoryCircular:
                VStack(spacing: 1) {
                    Text("Hooks").font(.caption2)
                    Text(record.view.hookCountText)
                        .font(.system(.title2, design: .rounded, weight: .semibold)).monospacedDigit()
                    Text("上次同步").font(.caption2)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(labels(record)?.appleAccessoryCircular ?? record.view.hooksText)
            case .accessoryRectangular:
                rectangular(record)
            case .systemMedium:
                medium(record).foregroundStyle(.white)
            default:
                small(record).foregroundStyle(.white)
            }
        } else {
            empty
        }
    }

    /// 小尺寸：总 Token；有额度时附各账户剩余额度，并压缩日期与同步时间为一行。
    private func small(_ record: WidgetContainer.Entry) -> some View {
        let hasQuota = record.view.quota != nil
        return VStack(alignment: .leading, spacing: hasQuota ? 2 : 4) {
            HStack {
                Text(hasQuota ? "Token" : "Token 用量")
                    .font(.system(size: hasQuota ? 9 : 11, weight: .medium)).foregroundStyle(quotaAccent)
                Spacer(minLength: 0)
                if !hasQuota { Image(systemName: "square.grid.2x2.fill").font(.caption2).foregroundStyle(accent) }
            }
            tokenValue(record, size: hasQuota ? 24 : 32, exactSize: hasQuota ? 8 : 10)
            if let quota = record.view.quota {
                quotaProviders(quota, view: record.view, compact: true)
            }
            Spacer(minLength: 0)
            if hasQuota {
                compactTimeSummary(record)
            } else {
                if let tokens = record.view.tokens {
                    Text(tokens.date).font(.system(size: 10)).foregroundStyle(.white.opacity(0.65))
                        .accessibilityLabel(tokens.dateAccessibility)
                }
                syncTime(record)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    /// 中尺寸：左侧总量，右侧额度（无额度时为前两个 Agent），底部 Hooks 摘要与同步时间。
    private func medium(_ record: WidgetContainer.Entry) -> some View {
        let hasQuota = record.view.quota != nil
        return VStack(alignment: .leading, spacing: hasQuota ? 6 : 7) {
            HStack {
                Text("Token 用量").font(.caption.weight(.medium)).foregroundStyle(accent)
                Spacer(minLength: 0)
                if let tokens = record.view.tokens {
                    Text(tokens.date).font(.caption2).foregroundStyle(.white.opacity(0.6))
                        .accessibilityLabel(tokens.dateAccessibility)
                }
            }
            HStack(alignment: .top, spacing: 14) {
                tokenValue(record, size: 34).frame(maxWidth: .infinity, alignment: .leading)
                Rectangle().fill(LinearGradient(colors: [accent.opacity(0.4), violet.opacity(0.1)], startPoint: .top, endPoint: .bottom)).frame(width: 0.5)
                if let quota = record.view.quota {
                    quotaProviders(quota, view: record.view).frame(maxWidth: .infinity, alignment: .leading)
                } else {
                    agents(record).frame(maxWidth: .infinity, alignment: .leading)
                }
            }.frame(maxHeight: .infinity, alignment: .top)
            HStack(alignment: .bottom, spacing: 8) {
                hookSummary(record)
                Spacer(minLength: 0)
                syncTime(record)
            }
        }.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    /// 待机使用的小尺寸也列出全部有效账户，每行并排呈现实际窗口，不用 +N 隐藏账户。
    private func quotaProviders(_ quota: DisplayView.Quota, view: DisplayView, compact: Bool = false) -> some View {
        let style = Date.FormatStyle.dateTime.locale(SyncTimeFormat.locale)
        let stamp = Calendar.current.isDate(quota.observedDate, inSameDayAs: entry.date)
            ? quota.observedDate.formatted(style.hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
            : quota.observedDate.formatted(style.month(.twoDigits).day(.twoDigits).hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
        let descriptions = try? view.labels(quotaTime: quota.observedDate.formatted(SyncTimeFormat.full), quotaCompactTime: stamp)
        return VStack(alignment: .leading, spacing: compact ? 2 : 4) {
            HStack(spacing: 3) {
                Text("剩余额度")
                Spacer(minLength: 0)
                Text(descriptions?.quotaCompactUpdate ?? "")
            }
            .font(.system(size: compact ? 7.5 : 8)).foregroundStyle(quotaSecondary)
            .lineLimit(1).minimumScaleFactor(0.8)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(descriptions?.quotaUpdate ?? "")
            ForEach(quota.providers, id: \.provider) { provider in
                HStack(alignment: .center, spacing: 4) {
                    ProviderQuotaTitle(name: provider.name, plan: provider.plan,
                                       font: .system(size: 9, weight: .semibold), compact: true)
                        .foregroundStyle(quotaAccent).frame(width: 59, alignment: .leading)
                    HStack(spacing: 5) {
                        ForEach(provider.windows, id: \.kind) { window in quotaWindow(window, compact: compact) }
                    }.frame(maxWidth: .infinity, alignment: .leading).layoutPriority(1)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(descriptions?.providerDescriptions[provider.provider] ?? provider.description)
            }
        }
    }

    private var quotaAccent: Color { renderingMode == .fullColor ? accent : .white }
    private var quotaSecondary: Color {
        renderingMode == .vibrant ? Color(white: 0.8) : .white.opacity(0.65)
    }
    private var quotaTrack: Color {
        renderingMode == .vibrant ? Color(white: 0.35) : .white.opacity(0.18)
    }
    /// 与共享模型阈值一致：剩余不超过 20% 为红、不超过 50% 为黄，其余为绿。
    private func quotaFill(_ window: DisplayView.Window) -> Color {
        guard renderingMode == .fullColor else { return .white }
        return window.level == 0 ? .red : window.level == 1 ? .yellow : .green
    }

    /// 静态三点细条属于内容层，待机移除背景后仍存在；单色模式用不透明灰阶保留对比。
    private func quotaWindow(_ window: DisplayView.Window, compact: Bool) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            HStack(spacing: 1) {
                Text(window.label).font(.system(size: compact ? 7.5 : 8))
                Spacer(minLength: 0)
                Text(window.percent).font(.system(size: compact ? 8 : 9, weight: .medium)).monospacedDigit()
                    .fixedSize(horizontal: true, vertical: false).layoutPriority(1)
            }
            GeometryReader { geometry in
                ZStack(alignment: .leading) {
                    Capsule().fill(quotaTrack)
                    Capsule().fill(quotaFill(window))
                        .frame(width: geometry.size.width * CGFloat(window.remainingTenths) / 1000)
                }
            }
            .frame(height: 3)
            .accessibilityHidden(true)
        }
        .frame(maxWidth: .infinity)
        .foregroundStyle(renderingMode == .vibrant ? Color.white : .white.opacity(0.85))
        .lineLimit(1).minimumScaleFactor(0.8)
    }

    @ViewBuilder private func agents(_ record: WidgetContainer.Entry) -> some View {
        let rows = record.view.agents
        if !rows.isEmpty {
            VStack(alignment: .leading, spacing: 3) {
                ForEach(Array(rows.prefix(2))) { row in
                    VStack(alignment: .leading, spacing: 1) {
                        HStack(alignment: .firstTextBaseline, spacing: 4) {
                            ProviderQuotaTitle(name: row.name, plan: row.plan, font: .caption2, compact: true)
                                .foregroundStyle(.white.opacity(0.65))
                            Spacer(minLength: 0)
                            Text(row.compact).font(.system(size: 14, weight: .semibold, design: .monospaced))
                                .monospacedDigit().lineLimit(1).minimumScaleFactor(0.4)
                                .layoutPriority(1)
                        }
                        if let exact = row.exact {
                            Text(exact).font(.system(size: 9, weight: .medium, design: .monospaced))
                                .monospacedDigit().foregroundStyle(.white.opacity(0.65))
                                .lineLimit(1).minimumScaleFactor(0.4)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(row.accessibilityLabel)
                }
                if let remaining = try? record.view.labels(visibleAgents: 2).remainingAgents, !remaining.isEmpty {
                    Text(remaining).font(.system(size: 9)).foregroundStyle(.white.opacity(0.6))
                }
            }
        } else {
            VStack(alignment: .leading, spacing: 3) {
                Text("Agent").font(.caption2).foregroundStyle(.white.opacity(0.65))
                Text(record.view.agentsEmptyText).font(.caption)
            }
        }
    }

    /// 一行 Hooks 摘要：上次同步的 Hook 数量与首个插槽的 Agent 和状态。
    private func hookSummary(_ record: WidgetContainer.Entry) -> some View {
        HStack(spacing: 5) {
            Image(systemName: "point.3.connected.trianglepath.dotted").foregroundStyle(accent)
            Text(record.view.hooksSummary)
            if let lead = record.view.hookLead {
                Text(lead).foregroundStyle(.white.opacity(0.65))
            }
        }.font(.system(size: 10)).lineLimit(1).minimumScaleFactor(0.7)
    }

    /// 锁屏矩形：统计日期、总量与上次同步时间。
    private func rectangular(_ record: WidgetContainer.Entry) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            HStack(spacing: 4) {
                Text("Token")
                if let tokens = record.view.tokens { Text(tokens.date) }
            }.font(.caption2).lineLimit(1).minimumScaleFactor(0.75)
            tokenValue(record, size: 20, exactSize: 9)
            Text(labels(record)?.appleSyncText ?? "")
                .font(.system(size: 9)).lineLimit(1).minimumScaleFactor(0.6)
        }
    }

    /// 缩略值仅用于大字扫读，下方完整整数不经过浮点转换。
    private func tokenValue(_ record: WidgetContainer.Entry, size: CGFloat, exactSize: CGFloat = 10) -> some View {
        let amount = record.view.tokens?.total
        let fallback = record.view.tokenFallback
        return VStack(alignment: .leading, spacing: 2) {
            Text(amount?.compact ?? fallback)
                .font(.system(size: size, weight: .semibold, design: .monospaced))
                .monospacedDigit().lineLimit(1).minimumScaleFactor(0.25)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let exact = amount?.exact {
                Text(exact).font(.system(size: exactSize, weight: .medium, design: .monospaced))
                    .monospacedDigit().opacity(0.65).lineLimit(1).minimumScaleFactor(0.5)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(record.view.tokenAccessibilityLabel)
    }

    /// 标明最近同步时间，不从系统时间推断在线或今日用量。
    private func syncTime(_ record: WidgetContainer.Entry, size: CGFloat = 10) -> some View {
        Text(labels(record)?.appleSyncText ?? "")
            .font(.system(size: size)).foregroundStyle(.white.opacity(0.6))
            .lineLimit(1).minimumScaleFactor(0.7)
            .accessibilityLabel(labels(record)?.appleSyncDescription ?? "")
    }

    /// 有额度时合并日期与同步行，为最小组件的有效高度保留余量；旧日同步仍披露日期。
    private func compactTimeSummary(_ record: WidgetContainer.Entry) -> some View {
        let style = Date.FormatStyle.dateTime.locale(SyncTimeFormat.locale)
        let stamp = Calendar.current.isDate(record.receivedDate, inSameDayAs: entry.date)
            ? record.receivedDate.formatted(style.hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
            : record.receivedDate.formatted(style.month(.twoDigits).day(.twoDigits).hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
        let text = labels(record, compactSyncTime: stamp)
        return Text(text?.appleCompactSyncText ?? "")
            .font(.system(size: 9)).foregroundStyle(.white.opacity(0.6))
            .lineLimit(1).minimumScaleFactor(0.75)
            .accessibilityLabel(text?.appleCompactSyncDescription ?? "")
    }

    private func labels(_ record: WidgetContainer.Entry, compactSyncTime: String? = nil) -> DisplayView.Labels? {
        try? record.view.labels(syncTime: cacheDescription(record), compactSyncTime: compactSyncTime ?? cacheStamp(record))
    }

    /// 简短的同步时间戳，用于空间受限的样式。
    private func cacheStamp(_ record: WidgetContainer.Entry) -> String {
        record.receivedDate.formatted(.dateTime.locale(SyncTimeFormat.locale).month(.twoDigits).day(.twoDigits).hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
    }

    /// 完整的同步时间文本，供读屏标签使用。
    private func cacheDescription(_ record: WidgetContainer.Entry) -> String {
        record.receivedDate.formatted(SyncTimeFormat.full)
    }

    /// 未同步不表示零用量，也不推断设备离线。
    @ViewBuilder private var empty: some View {
        let labels = DisplayView.emptyLabels(appName: appDisplayName)
        switch family {
        case .accessoryInline:
            Text(labels?.widgetEmptyInline ?? "")
        case .accessoryCircular:
            VStack(spacing: 2) {
                Image(systemName: "square.grid.2x2")
                Text(labels?.widgetEmptyCircular ?? "").font(.caption2)
            }
        case .accessoryRectangular:
            VStack(alignment: .leading, spacing: 3) {
                Text(appDisplayName).font(.caption2)
                Text(labels?.widgetEmptyTitle ?? "").font(.headline)
            }
        default:
            VStack(alignment: .leading, spacing: 8) {
                Image(systemName: "square.grid.2x2.fill").font(.title3).foregroundStyle(accent)
                Spacer(minLength: 0)
                Text(labels?.widgetEmptyTitle ?? "").font(.headline)
                Text(labels?.widgetEmptyHelp ?? "").font(.caption2).foregroundStyle(.white.opacity(0.65))
            }
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        }
    }
}

/// WidgetKit仅绘制静态材质；锁屏交还系统单色背景，不运行持续动画。
private struct WidgetCyberMaterial: View {
    @Environment(\.widgetFamily) private var family
    private let cyan = Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255)
    private let violet = Color(red: 174 / 255, green: 140 / 255, blue: 255 / 255)

    @ViewBuilder var body: some View {
        switch family {
        case .accessoryInline, .accessoryCircular, .accessoryRectangular:
            Color.clear
        default:
            ZStack {
                LinearGradient(colors: [Color(red: 17 / 255, green: 29 / 255, blue: 45 / 255), Color(red: 5 / 255, green: 10 / 255, blue: 18 / 255)], startPoint: .topLeading, endPoint: .bottomTrailing)
                WidgetCircuitGrid().stroke(cyan.opacity(0.055), lineWidth: 0.5)
                RadialGradient(colors: [cyan.opacity(0.11), .clear], center: .topLeading, startRadius: 0, endRadius: 130)
                RadialGradient(colors: [violet.opacity(0.1), .clear], center: .bottomTrailing, startRadius: 0, endRadius: 110)
                ContainerRelativeShape().strokeBorder(LinearGradient(colors: [cyan.opacity(0.5), .white.opacity(0.04), violet.opacity(0.35)], startPoint: .topLeading, endPoint: .bottomTrailing), lineWidth: 0.8)
            }
            .accessibilityHidden(true)
        }
    }
}

private struct WidgetCircuitGrid: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        for x in stride(from: CGFloat(0), through: rect.width, by: 18) {
            path.move(to: CGPoint(x: x, y: 0)); path.addLine(to: CGPoint(x: x, y: rect.height))
        }
        for y in stride(from: CGFloat(0), through: rect.height, by: 18) {
            path.move(to: CGPoint(x: 0, y: y)); path.addLine(to: CGPoint(x: rect.width, y: y))
        }
        return path
    }
}
