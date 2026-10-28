// 引入 SwiftUI 与 WidgetKit
import SwiftUI
// 引入 WidgetKit
import WidgetKit

/// 五种尺寸共用系统Timeline，系统唤醒时使用独立只读权限短时拉取摘要。
// 小组件入口：五种尺寸共用系统 Timeline，系统唤醒时用独立的只读权限短时拉取摘要
@main
// 小组件声明
struct LokiMetrisWidgets: Widget {
    // 小组件配置
    var body: some WidgetConfiguration {
        // 静态配置：使用统一的容器背景，内容标记为隐私敏感
        StaticConfiguration(kind: WidgetContainer.kind, provider: DisplayTimeline()) { entry in
            // 小组件视图
            DisplayWidgetView(entry: entry)
                // 容器背景使用赛博材质
                .containerBackground(for: .widget) { WidgetCyberMaterial() }
                // 隐私敏感
                .privacySensitive()
        }
        // 配置界面里的显示名
        .configurationDisplayName("app_display_name")
        // 配置界面里的说明文字
        .description("上次同步的 Token 用量、剩余额度与 Hooks 摘要。")
        // 支持的尺寸：小、中，以及锁屏的内联、圆形、矩形
        .supportedFamilies([.systemSmall, .systemMedium, .accessoryInline, .accessoryCircular, .accessoryRectangular])
    }
}

/// Timeline 条目；snapshot 为空时显示未同步占位。
// Timeline 条目；snapshot 为空时显示“未同步”占位
struct DisplayEntry: TimelineEntry {
    // 条目的日期
    let date: Date
    // 容器里读取的快照
    let snapshot: WidgetContainer.Entry?
}

/// 本地缓存优先；Timeline唤醒时独立HTTP读取，系统决定实际执行时间。
// 本地缓存优先；Timeline 唤醒时独立 HTTP 读取，系统决定实际执行时间
struct DisplayTimeline: TimelineProvider {
    /// 系统占位：不读磁盘，直接显示未同步状态。
    // 系统占位：不读磁盘，直接显示未同步状态
    func placeholder(in context: Context) -> DisplayEntry { DisplayEntry(date: Date(), snapshot: nil) }
    /// 小组件库预览与快速展示只读本地缓存，不发网络请求。
    // 小组件库预览与快速展示只读本地缓存，不发网络请求
    func getSnapshot(in context: Context, completion: @escaping (DisplayEntry) -> Void) { completion(load()) }
    /// 有凭据时先 HTTP 拉取并落盘，再按 `WidgetContainer.refreshInterval` 申请下次刷新。
    /// 摘要仍在 45 秒租约内时跳过 HTTP；App 心跳可续租，HTTP 刚拉取的摘要也有初始租约。
    /// 这只用于抑制重复请求，不在组件界面中作为桌面实时在线的证明。
    // 有凭据时先 HTTP 拉取并落盘，再按刷新间隔申请下次刷新；摘要仍在 45 秒租约内则跳过 HTTP。这只用来抑制重复请求，不在界面里当作桌面实时在线的证明
    func getTimeline(in context: Context, completion: @escaping (Timeline<DisplayEntry>) -> Void) {
        // 启动异步任务
        Task {
            // 租约内的摘要由 Rust 判断并跳过请求。
            // 预览不发请求；租约内的摘要由 Rust 判断并跳过
            if !context.isPreview { _ = await WidgetContainer.refresh(origin: "timeline") }
            // 加载条目
            let entry = load()
            // 时间线：单个条目，到期后刷新
            completion(Timeline(entries: [entry], policy: .after(Date(timeIntervalSinceNow: WidgetContainer.refreshInterval))))
        }
    }
    /// 读取最近一次合法摘要；过期仍显示，并标注真实同步时间。
    // 读取最近一次合法摘要；过期仍显示，并标注真实的同步时间
    func load() -> DisplayEntry {
        // 当前时间
        let now = Date()
        // 构造条目并加载快照
        return DisplayEntry(date: now, snapshot: WidgetContainer.load(atMs: now.epochMs))
    }
}

/// 每种组件各自组织信息；锁屏保持系统单色，桌面使用高对比数字。
// 每种组件各自组织信息：锁屏保持系统单色，桌面使用高对比数字
struct DisplayWidgetView: View {
    // 当前尺寸与渲染模式
    @Environment(\.widgetFamily) private var family
    // 读取渲染模式
    @Environment(\.widgetRenderingMode) var renderingMode
    // 主强调色（青）与次强调色（紫）
    let accent = Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255)
    // 紫色强调色
    let violet = Color(red: 174 / 255, green: 140 / 255, blue: 255 / 255)
    // 条目
    let entry: DisplayEntry

    // 应用显示名
    var appDisplayName: String {
        // 应用显示名，取自 Info.plist
        Bundle.main.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String ?? "LokiMetris"
    }

    // 视图主体
    @ViewBuilder var body: some View {
        // 有缓存：按尺寸选择布局
        if let record = entry.snapshot {
            // 按小组件尺寸切换
            switch family {
            // 锁屏内联：一行文字
            case .accessoryInline:
                // 锁屏内联：读屏文案或 Hooks 文本
                Text(labels(record)?.appleAccessoryInline ?? record.view.hooksText)
            // 锁屏圆形：Hooks 数量与“上次同步”
            case .accessoryCircular:
                // 锁屏圆形：纵向排列
                VStack(spacing: 1) {
                    // 标题 Hooks
                    Text("Hooks").font(.caption2)
                    // Hook 数量文本
                    Text(record.view.hookCountText)
                        // 圆角字体、等宽数字
                        .font(.system(.title2, design: .rounded, weight: .semibold)).monospacedDigit()
                    // “上次同步”标题
                    Text("上次同步").font(.caption2)
                }
                // 忽略子元素的读屏
                .accessibilityElement(children: .ignore)
                // 读屏文案
                .accessibilityLabel(labels(record)?.appleAccessoryCircular ?? record.view.hooksText)
            // 锁屏矩形
            case .accessoryRectangular:
                // 锁屏矩形
                rectangular(record)
            // 中尺寸
            case .systemMedium:
                // 中尺寸，白色前景
                medium(record).foregroundStyle(.white)
            // 小尺寸
            default:
                // 小尺寸，白色前景
                small(record).foregroundStyle(.white)
            }
        // 没有缓存：显示空态
        } else {
            // 没有数据时显示空态
            empty
        }
    }

    /// 小尺寸：总 Token；有额度时附各账户剩余额度，并压缩日期与同步时间为一行。
    // 小尺寸：总 Token；有额度时附各账户剩余额度，并把日期与同步时间压缩为一行
    func small(_ record: WidgetContainer.Entry) -> some View {
        // 是否有额度数据
        let hasQuota = record.view.quota != nil
        // 纵向排列，间距随是否有额度而变
        return VStack(alignment: .leading, spacing: hasQuota ? 2 : 4) {
            // 标题行：有额度时缩小，无额度时带图标
            HStack {
                // 标题：有额度时只写 Token
                Text(hasQuota ? "Token" : "Token 用量")
                    // 字号随是否有额度而变
                    .font(.system(size: hasQuota ? 9 : 11, weight: .medium)).foregroundStyle(quotaAccent)
                // 弹性占位
                Spacer(minLength: 0)
                // 没有额度时显示图标
                if !hasQuota { Image(systemName: "square.grid.2x2.fill").font(.caption2).foregroundStyle(accent) }
            }
            // 总量
            tokenValue(record, size: hasQuota ? 24 : 32, exactSize: hasQuota ? 8 : 10)
            // 有额度则列出各账户
            if let quota = record.view.quota {
                // 额度供应商（紧凑）
                quotaProviders(quota, view: record.view, compact: true)
            }
            // 弹性占位
            Spacer(minLength: 0)
            // 有额度：合并日期与同步行
            if hasQuota {
                // 紧凑的时间摘要
                compactTimeSummary(record)
            // 无额度：分别显示日期与同步时间
            } else {
                // 有 Token 数据时
                if let tokens = record.view.tokens {
                    // 显示日期
                    Text(tokens.date).font(.system(size: 10)).foregroundStyle(.white.opacity(0.65))
                        // 读屏文案：日期
                        .accessibilityLabel(tokens.dateAccessibility)
                }
                // 同步时间
                syncTime(record)
            }
        }
        // 占满并左上对齐
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    /// 中尺寸：左侧总量，右侧额度（无额度时为前两个 Agent），底部 Hooks 摘要与同步时间。
    // 中尺寸：左侧总量，右侧额度（无额度时为前两个 Agent），底部为 Hooks 摘要与同步时间
    func medium(_ record: WidgetContainer.Entry) -> some View {
        // 是否有额度数据
        let hasQuota = record.view.quota != nil
        // 纵向排列，间距随是否有额度而变
        return VStack(alignment: .leading, spacing: hasQuota ? 6 : 7) {
            // 标题行：标题与统计日期
            HStack {
                // 标题 Token 用量
                Text("Token 用量").font(.caption.weight(.medium)).foregroundStyle(accent)
                // 弹性占位
                Spacer(minLength: 0)
                // 有 Token 数据
                if let tokens = record.view.tokens {
                    // 日期
                    Text(tokens.date).font(.caption2).foregroundStyle(.white.opacity(0.6))
                        // 读屏文案：日期
                        .accessibilityLabel(tokens.dateAccessibility)
                }
            }
            // 主体：总量、渐变分隔线与右侧内容
            HStack(alignment: .top, spacing: 14) {
                // Token 总量（大字号）
                tokenValue(record, size: 34).frame(maxWidth: .infinity, alignment: .leading)
                // 渐变分隔线
                Rectangle().fill(LinearGradient(colors: [accent.opacity(0.4), violet.opacity(0.1)], startPoint: .top, endPoint: .bottom)).frame(width: 0.5)
                // 有额度数据
                if let quota = record.view.quota {
                    // 额度供应商
                    quotaProviders(quota, view: record.view).frame(maxWidth: .infinity, alignment: .leading)
                // 否则分支
                } else {
                    // 代理明细
                    agents(record).frame(maxWidth: .infinity, alignment: .leading)
                }
            // 顶部对齐
            }.frame(maxHeight: .infinity, alignment: .top)
            // 底部：Hooks 摘要与同步时间
            HStack(alignment: .bottom, spacing: 8) {
                // Hook 摘要
                hookSummary(record)
                // 弹性占位
                Spacer(minLength: 0)
                // 同步时间
                syncTime(record)
            }
        // 占满并左上对齐
        }.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }


    // 无额度时中尺寸右侧显示前两个 Agent
    @ViewBuilder func agents(_ record: WidgetContainer.Entry) -> some View {
        // 代理行列表
        let rows = record.view.agents
        // 有代理行时
        if !rows.isEmpty {
            // 纵向排列
            VStack(alignment: .leading, spacing: 3) {
                // 每行：名称、紧凑值与可选的精确值
                ForEach(Array(rows.prefix(2))) { row in
                    // 纵向排列
                    VStack(alignment: .leading, spacing: 1) {
                        // 横向排列，按首行基线对齐
                        HStack(alignment: .firstTextBaseline, spacing: 4) {
                            // 供应商标题
                            ProviderQuotaTitle(name: row.name, plan: row.plan, font: .caption2, compact: true)
                                // 弱化颜色
                                .foregroundStyle(.white.opacity(0.65))
                            // 弹性占位
                            Spacer(minLength: 0)
                            // 紧凑数值
                            Text(row.compact).font(.system(size: 14, weight: .semibold, design: .monospaced))
                                // 等宽数字、单行并允许缩小
                                .monospacedDigit().lineLimit(1).minimumScaleFactor(0.4)
                                // 优先保证紧凑值的显示
                                .layoutPriority(1)
                        }
                        // 有精确值才显示
                        if let exact = row.exact {
                            // 精确值文字
                            Text(exact).font(.system(size: 9, weight: .medium, design: .monospaced))
                                // 等宽数字、弱化颜色
                                .monospacedDigit().foregroundStyle(.white.opacity(0.65))
                                // 单行并允许缩小
                                .lineLimit(1).minimumScaleFactor(0.4)
                                // 左对齐占满
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                    // 合并为一个读屏元素
                    .accessibilityElement(children: .ignore)
                    // 读屏文案
                    .accessibilityLabel(row.accessibilityLabel)
                }
                // 被隐藏的 Agent 数量提示
                if let remaining = try? record.view.labels(visibleAgents: 2).remainingAgents, !remaining.isEmpty {
                    // 剩余代理数提示
                    Text(remaining).font(.system(size: 9)).foregroundStyle(.white.opacity(0.6))
                }
            }
        // 没有明细时显示占位说明
        } else {
            // 纵向排列
            VStack(alignment: .leading, spacing: 3) {
                // 标题 Agent
                Text("Agent").font(.caption2).foregroundStyle(.white.opacity(0.65))
                // 无代理时的占位文案
                Text(record.view.agentsEmptyText).font(.caption)
            }
        }
    }

    /// 一行 Hooks 摘要：上次同步的 Hook 数量与首个插槽的 Agent 和状态。
    // 一行 Hooks 摘要：上次同步的 Hook 数量，以及首个插槽的 Agent 与状态
    func hookSummary(_ record: WidgetContainer.Entry) -> some View {
        // 横向排列
        HStack(spacing: 5) {
            // 连接图标
            Image(systemName: "point.3.connected.trianglepath.dotted").foregroundStyle(accent)
            // Hook 摘要
            Text(record.view.hooksSummary)
            // 有首个 Hook 时
            if let lead = record.view.hookLead {
                // 引导文字
                Text(lead).foregroundStyle(.white.opacity(0.65))
            }
        // 字号、单行并允许缩小
        }.font(.system(size: 10)).lineLimit(1).minimumScaleFactor(0.7)
    }

    /// 锁屏矩形：统计日期、总量与上次同步时间。
    // 锁屏矩形：统计日期、总量与上次同步时间
    func rectangular(_ record: WidgetContainer.Entry) -> some View {
        // 纵向排列
        VStack(alignment: .leading, spacing: 1) {
            // 横向排列
            HStack(spacing: 4) {
                // 标题 Token
                Text("Token")
                // 有 Token 数据时显示日期
                if let tokens = record.view.tokens { Text(tokens.date) }
            // 次要字号、单行并允许缩小
            }.font(.caption2).lineLimit(1).minimumScaleFactor(0.75)
            // Token 数值
            tokenValue(record, size: 20, exactSize: 9)
            // 同步文案
            Text(labels(record)?.appleSyncText ?? "")
                // 小字号、单行并允许缩小
                .font(.system(size: 9)).lineLimit(1).minimumScaleFactor(0.6)
        }
    }

    /// 缩略值仅用于大字扫读，下方完整整数不经过浮点转换。
    // 总量：缩略值只用来大字扫读，下方的完整整数不经过浮点转换
    func tokenValue(_ record: WidgetContainer.Entry, size: CGFloat, exactSize: CGFloat = 10) -> some View {
        // Token 总量
        let amount = record.view.tokens?.total
        // 备用文案
        let fallback = record.view.tokenFallback
        // 纵向排列
        return VStack(alignment: .leading, spacing: 2) {
            // 紧凑写法，缺失时用备用文案
            Text(amount?.compact ?? fallback)
                // 等宽字体
                .font(.system(size: size, weight: .semibold, design: .monospaced))
                // 等宽数字、单行并允许缩小
                .monospacedDigit().lineLimit(1).minimumScaleFactor(0.25)
                // 左对齐占满
                .frame(maxWidth: .infinity, alignment: .leading)
            // 有精确值才显示
            if let exact = amount?.exact {
                // 精确值文字
                Text(exact).font(.system(size: exactSize, weight: .medium, design: .monospaced))
                    // 等宽数字、弱化并允许缩小
                    .monospacedDigit().opacity(0.65).lineLimit(1).minimumScaleFactor(0.5)
                    // 左对齐占满
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        // 合并为一个读屏元素
        .accessibilityElement(children: .ignore)
        // 读屏文案
        .accessibilityLabel(record.view.tokenAccessibilityLabel)
    }

    /// 标明最近同步时间，不从系统时间推断在线或今日用量。
    // 标明最近同步时间，不从系统时间推断在线或今日用量
    func syncTime(_ record: WidgetContainer.Entry, size: CGFloat = 10) -> some View {
        // 同步状态文案
        Text(labels(record)?.appleSyncText ?? "")
            // 字号与弱化颜色
            .font(.system(size: size)).foregroundStyle(.white.opacity(0.6))
            // 单行并允许缩小
            .lineLimit(1).minimumScaleFactor(0.7)
            // 读屏文案：同步描述
            .accessibilityLabel(labels(record)?.appleSyncDescription ?? "")
    }

    /// 有额度时合并日期与同步行，为最小组件的有效高度保留余量；旧日同步仍披露日期。
    // 有额度时合并日期与同步行，为最小组件的有效高度留出余量；旧日同步仍披露日期
    func compactTimeSummary(_ record: WidgetContainer.Entry) -> some View {
        // 日期格式样式
        let style = Date.FormatStyle.dateTime.locale(SyncTimeFormat.locale)
        // 同一天只显示时分
        let stamp = Calendar.current.isDate(record.receivedDate, inSameDayAs: entry.date)
            // 同一天的写法
            ? record.receivedDate.formatted(style.hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
            // 否则显示月日
            : record.receivedDate.formatted(style.month(.twoDigits).day(.twoDigits).hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
        // 紧凑同步文案
        let text = labels(record, compactSyncTime: stamp)
        // 返回文本
        return Text(text?.appleCompactSyncText ?? "")
            // 小字号与弱化颜色
            .font(.system(size: 9)).foregroundStyle(.white.opacity(0.6))
            // 单行并允许缩小
            .lineLimit(1).minimumScaleFactor(0.75)
            // 读屏文案
            .accessibilityLabel(text?.appleCompactSyncDescription ?? "")
    }

    // 生成同步相关文案：日期由 Foundation 按本机区域格式化
    func labels(_ record: WidgetContainer.Entry, compactSyncTime: String? = nil) -> DisplayView.Labels? {
        // 生成标签，带同步时间描述
        try? record.view.labels(syncTime: cacheDescription(record), compactSyncTime: compactSyncTime ?? cacheStamp(record))
    }

    /// 简短的同步时间戳，用于空间受限的样式。
    // 简短的同步时间戳，用于空间受限的样式
    func cacheStamp(_ record: WidgetContainer.Entry) -> String {
        // 格式化接收日期：月、日等
        record.receivedDate.formatted(.dateTime.locale(SyncTimeFormat.locale).month(.twoDigits).day(.twoDigits).hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
    }

    /// 完整的同步时间文本，供读屏标签使用。
    // 完整的同步时间文本，供读屏标签使用
    func cacheDescription(_ record: WidgetContainer.Entry) -> String {
        // 完整格式的接收日期
        record.receivedDate.formatted(SyncTimeFormat.full)
    }

    /// 未同步不表示零用量，也不推断设备离线。
    // 空态：未同步不表示零用量，也不推断设备离线
    @ViewBuilder var empty: some View {
        // 空态文案
        let labels = DisplayView.emptyLabels(appName: appDisplayName)
        // 按小组件尺寸切换
        switch family {
        // 锁屏内联的空态
        case .accessoryInline:
            // 文字
            Text(labels?.widgetEmptyInline ?? "")
        // 锁屏圆形
        case .accessoryCircular:
            // 圆形空态：纵向排列
            VStack(spacing: 2) {
                // 图标
                Image(systemName: "square.grid.2x2")
                // 圆形空态的文字
                Text(labels?.widgetEmptyCircular ?? "").font(.caption2)
            }
        // 矩形空态
        case .accessoryRectangular:
            // 纵向排列
            VStack(alignment: .leading, spacing: 3) {
                // 应用名
                Text(appDisplayName).font(.caption2)
                // 空态标题
                Text(labels?.widgetEmptyTitle ?? "").font(.headline)
            }
        // 桌面尺寸的空态
        default:
            // 纵向排列
            VStack(alignment: .leading, spacing: 8) {
                // 图标
                Image(systemName: "square.grid.2x2.fill").font(.title3).foregroundStyle(accent)
                // 弹性占位
                Spacer(minLength: 0)
                // 空态标题
                Text(labels?.widgetEmptyTitle ?? "").font(.headline)
                // 帮助文字
                Text(labels?.widgetEmptyHelp ?? "").font(.caption2).foregroundStyle(.white.opacity(0.65))
            }
            // 白色前景
            .foregroundStyle(.white)
            // 占满并左对齐
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        }
    }
}
