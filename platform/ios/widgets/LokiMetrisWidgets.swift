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
    private func load() -> DisplayEntry {
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
    @Environment(\.widgetRenderingMode) private var renderingMode
    // 主强调色（青）与次强调色（紫）
    private let accent = Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255)
    // 紫色强调色
    private let violet = Color(red: 174 / 255, green: 140 / 255, blue: 255 / 255)
    // 条目
    let entry: DisplayEntry

    // 应用显示名
    private var appDisplayName: String {
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
    private func small(_ record: WidgetContainer.Entry) -> some View {
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
    private func medium(_ record: WidgetContainer.Entry) -> some View {
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

    /// 待机使用的小尺寸也列出全部有效账户，每行并排呈现实际窗口，不用 +N 隐藏账户。
    // 列出全部有效账户，每行并排呈现实际窗口，不用 +N 隐藏账户（待机用的小尺寸也一样）
    private func quotaProviders(_ quota: DisplayView.Quota, view: DisplayView, compact: Bool = false) -> some View {
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
    private var quotaAccent: Color { renderingMode == .fullColor ? accent : .white }
    // 次要文字颜色
    private var quotaSecondary: Color {
        // 鲜艳模式下用浅灰，否则半透明白
        renderingMode == .vibrant ? Color(white: 0.8) : .white.opacity(0.65)
    }
    // 进度条底色
    private var quotaTrack: Color {
        // 鲜艳模式下用深灰，否则淡白
        renderingMode == .vibrant ? Color(white: 0.35) : .white.opacity(0.18)
    }
    /// 与共享模型阈值一致：剩余不超过 20% 为红、不超过 50% 为黄，其余为绿。
    // 与共享模型的阈值一致：剩余不超过 20% 为红、不超过 50% 为黄，其余为绿
    private func quotaFill(_ window: DisplayView.Window) -> Color {
        // 非全彩模式一律用白色
        guard renderingMode == .fullColor else { return .white }
        // 按级别取色：红、黄、绿
        return window.level == 0 ? .red : window.level == 1 ? .yellow : .green
    }

    /// 静态三点细条属于内容层，待机移除背景后仍存在；单色模式用不透明灰阶保留对比。
    // 单个额度窗口：标签、百分比和一条静态细条；细条属于内容层，待机移除背景后仍存在
    private func quotaWindow(_ window: DisplayView.Window, compact: Bool) -> some View {
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

    // 无额度时中尺寸右侧显示前两个 Agent
    @ViewBuilder private func agents(_ record: WidgetContainer.Entry) -> some View {
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
    private func hookSummary(_ record: WidgetContainer.Entry) -> some View {
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
    private func rectangular(_ record: WidgetContainer.Entry) -> some View {
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
    private func tokenValue(_ record: WidgetContainer.Entry, size: CGFloat, exactSize: CGFloat = 10) -> some View {
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
    private func syncTime(_ record: WidgetContainer.Entry, size: CGFloat = 10) -> some View {
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
    private func compactTimeSummary(_ record: WidgetContainer.Entry) -> some View {
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
    private func labels(_ record: WidgetContainer.Entry, compactSyncTime: String? = nil) -> DisplayView.Labels? {
        // 生成标签，带同步时间描述
        try? record.view.labels(syncTime: cacheDescription(record), compactSyncTime: compactSyncTime ?? cacheStamp(record))
    }

    /// 简短的同步时间戳，用于空间受限的样式。
    // 简短的同步时间戳，用于空间受限的样式
    private func cacheStamp(_ record: WidgetContainer.Entry) -> String {
        // 格式化接收日期：月、日等
        record.receivedDate.formatted(.dateTime.locale(SyncTimeFormat.locale).month(.twoDigits).day(.twoDigits).hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
    }

    /// 完整的同步时间文本，供读屏标签使用。
    // 完整的同步时间文本，供读屏标签使用
    private func cacheDescription(_ record: WidgetContainer.Entry) -> String {
        // 完整格式的接收日期
        record.receivedDate.formatted(SyncTimeFormat.full)
    }

    /// 未同步不表示零用量，也不推断设备离线。
    // 空态：未同步不表示零用量，也不推断设备离线
    @ViewBuilder private var empty: some View {
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

/// WidgetKit仅绘制静态材质；锁屏交还系统单色背景，不运行持续动画。
// 背景材质：WidgetKit 只绘制静态材质；锁屏交还系统的单色背景，不运行持续动画
private struct WidgetCyberMaterial: View {
    // 读取小组件尺寸
    @Environment(\.widgetFamily) private var family
    // 青色强调色
    private let cyan = Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255)
    // 紫色强调色
    private let violet = Color(red: 174 / 255, green: 140 / 255, blue: 255 / 255)

    // 视图主体
    @ViewBuilder var body: some View {
        // 按尺寸选择背景
        switch family {
        // 锁屏：透明，由系统绘制
        case .accessoryInline, .accessoryCircular, .accessoryRectangular:
            // 透明底
            Color.clear
        // 桌面：渐变底、网格、两处辉光与渐变描边
        default:
            // 叠放
            ZStack {
                // 深蓝渐变底
                LinearGradient(colors: [Color(red: 17 / 255, green: 29 / 255, blue: 45 / 255), Color(red: 5 / 255, green: 10 / 255, blue: 18 / 255)], startPoint: .topLeading, endPoint: .bottomTrailing)
                // 电路网格
                WidgetCircuitGrid().stroke(cyan.opacity(0.055), lineWidth: 0.5)
                // 左上青色辉光
                RadialGradient(colors: [cyan.opacity(0.11), .clear], center: .topLeading, startRadius: 0, endRadius: 130)
                // 右下紫色辉光
                RadialGradient(colors: [violet.opacity(0.1), .clear], center: .bottomTrailing, startRadius: 0, endRadius: 110)
                // 渐变描边
                ContainerRelativeShape().strokeBorder(LinearGradient(colors: [cyan.opacity(0.5), .white.opacity(0.04), violet.opacity(0.35)], startPoint: .topLeading, endPoint: .bottomTrailing), lineWidth: 0.8)
            }
            // 对读屏隐藏
            .accessibilityHidden(true)
        }
    }
}

// 电路网格形状：每 18 点一条横竖线
private struct WidgetCircuitGrid: Shape {
    // 网格路径
    func path(in rect: CGRect) -> Path {
        // 新建路径
        var path = Path()
        // 竖线
        for x in stride(from: CGFloat(0), through: rect.width, by: 18) {
            // 竖线
            path.move(to: CGPoint(x: x, y: 0)); path.addLine(to: CGPoint(x: x, y: rect.height))
        }
        // 横线
        for y in stride(from: CGFloat(0), through: rect.height, by: 18) {
            // 横线
            path.move(to: CGPoint(x: 0, y: y)); path.addLine(to: CGPoint(x: rect.width, y: y))
        }
        // 返回路径
        return path
    }
}
