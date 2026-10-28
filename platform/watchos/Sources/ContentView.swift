// 引入 SwiftUI
import SwiftUI

/// Hooks、Token、额度从左到右独立分页；首次显示Token，以语义ID记住上次页面。
@MainActor
// 主界面：Hooks、Token、额度从左到右独立分页，首次显示 Token，以语义 ID 记住上次页面
struct ContentView: View {
    // 三个页面
    private enum Page: String, Hashable, CaseIterable { case hooks, tokens, quota }
    // 面板底色
    private let panel = Color(red: 17 / 255, green: 29 / 255, blue: 45 / 255)
    // 强调色
    private let accent = Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255)
    // 环境：场景阶段、减弱动态效果、低亮度
    @Environment(\.scenePhase) private var phase
    // 读取“减少动态效果”设置
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    // 读取“降低亮度”状态
    @Environment(\.isLuminanceReduced) private var reducedLuminance
    // 数据模型
    @ObservedObject var model: WatchModel
    // 入场动画状态
    @State private var entered = false
    // 数据更新时的边框高亮
    @State private var updateGlow = false
    // 高亮触发计数
    @State private var glowRevision = 0
    // 当前可见的 Hook 槽位
    @State private var visibleHookSlot: Int?
    // 打开详情的 Hook
    @State private var selectedHook: HookItem?
    // 记住上次所在页面
    @AppStorage("watch.panel.selectedPage") private var selectedPage = Page.tokens.rawValue

    // 只有前台、未减弱动态效果且未低亮度时才允许动画
    private var allowsMotion: Bool {
        // 活动状态且未减少动效、未降亮度才允许动效
        phase == .active && !reduceMotion && !reducedLuminance
    }

    // 视图主体
    var body: some View {
        // 仪表盘加详情弹窗
        dashboard
            // 点击 Hook 弹出详情
            .sheet(item: $selectedHook) { hook in
                // 详情页使用导航栈
                NavigationStack { HookDetailView(hook: hook, model: model) }
            }
    }

    // 三页横向分页的主体
    private var dashboard: some View {
        // 以外层页面尺寸为准
        GeometryReader { pageArea in
            // 分页视图，绑定选中页
            TabView(selection: $selectedPage) {
                // Hook 页，并打上页码标签
                hooksPage(size: pageArea.size).tag(Page.hooks.rawValue)
                // Token 页
                tokensPage.tag(Page.tokens.rawValue)
                // 额度页
                quotaPage.tag(Page.quota.rawValue)
            }
            // 页面大小与可用区域一致
            .frame(width: pageArea.size.width, height: pageArea.size.height)
            // 分页，隐藏系统圆点（下面自绘）
            .tabViewStyle(.page(indexDisplayMode: .never))
            // 测试标识
            .accessibilityIdentifier("watch-pages")
            // 底部自绘的分页圆点
            .overlay(alignment: .bottom) {
                // 横向排列的页码圆点
                HStack(spacing: 5) {
                    // 每页一个圆点
                    ForEach(Page.allCases, id: \.rawValue) { page in
                        // 选中页的圆点更亮
                        Circle().fill(selectedPage == page.rawValue ? .white : .white.opacity(0.35))
                            // 圆点大小 4
                            .frame(width: 4, height: 4)
                    }
                }
                // 圆点区域高度 4
                .frame(height: 4)
                // 为分页圆点留出底部间距
                .padding(.bottom, 5)
                // 圆点不接收点击
                .allowsHitTesting(false)
                // 对读屏隐藏
                .accessibilityHidden(true)
            }
        }
        // 延伸到底部安全区
        .ignoresSafeArea(.container, edges: .bottom)
        // 入场时轻微淡入和缩放
        .opacity(entered || !allowsMotion ? 1 : 0.5)
        // 进场时轻微缩放
        .scaleEffect(entered || !allowsMotion ? 1 : 0.98, anchor: .top)
        // 黑色背景
        .background(Color.black)
        // 强调色
        .tint(accent)
        // 保存的页面无效则回到 Token
        .onAppear {
            // 选中页无效时回到 Token 页
            if Page(rawValue: selectedPage) == nil { selectedPage = Page.tokens.rawValue }
        }
        // Hook 槽位变化时，清掉已不存在的可见槽位；初始位置交给滚动容器报告，避免程序定位覆盖系统安全区
        .onChange(of: model.manifest?.snapshot?.hooks.map(\.slot), initial: true) { _, slots in
            // 没有槽位则清除当前可见槽位
            guard let slots, !slots.isEmpty else { visibleHookSlot = nil; return }
            // 初始位置交给滚动容器报告，避免恢复 Hooks 页时程序定位覆盖系统安全区。
            if let slot = visibleHookSlot, !slots.contains(slot) { visibleHookSlot = nil }
        }
        // 回到前台且允许动画时触发高亮，否则停止动画
        .onChange(of: phase, initial: true) { _, value in
            // 回到活动状态
            if value == .active {
                // 允许动效时
                if allowsMotion {
                    // 光效版本加一，触发一次光效
                    glowRevision += 1
                // 否则停止动效
                } else { stopMotion() }
            // 否则停止动效
            } else { stopMotion() }
        }
        // 快照变化时触发高亮
        .onChange(of: model.manifest?.snapshot) { old, new in
            // 数据变化且允许动效时触发光效
            if old != new, allowsMotion { glowRevision += 1 }
        }
        // 动态设置变化时停止动画
        .onChange(of: reduceMotion) { _, _ in if !allowsMotion { stopMotion() } }
        // 亮度降低后若不再允许动效则停止
        .onChange(of: reducedLuminance) { _, _ in if !allowsMotion { stopMotion() } }
        // 前台入场动画：先无动画复位，稍等再淡入
        .task(id: phase) {
            // 不允许动效则直接返回
            guard allowsMotion else { return }
            // 新建事务，用于关闭隐式动画
            var transaction = Transaction()
            // 关闭隐式动画
            transaction.disablesAnimations = true
            // 在事务里重置进场状态
            withTransaction(transaction) { entered = false }
            // 等待 35 毫秒
            do { try await Task.sleep(for: .milliseconds(35)) }
            // 任务被取消则返回
            catch { return }
            // 等待期间动效被关闭则返回
            guard allowsMotion else { return }
            // 淡入完成
            withAnimation(.easeOut(duration: 0.3)) { entered = true }
        }
        // 高亮短暂亮起再熄灭
        .task(id: glowRevision) {
            // 不允许动效则直接返回
            guard allowsMotion else { return }
            // 0.2 秒内亮起光效
            withAnimation(.easeOut(duration: 0.2)) { updateGlow = true }
            // 保持 450 毫秒
            do { try await Task.sleep(for: .milliseconds(450)) }
            // 任务被取消则返回
            catch { return }
            // 动效被关闭则返回
            guard allowsMotion else { return }
            // 0.35 秒内熄灭光效
            withAnimation(.easeOut(duration: 0.35)) { updateGlow = false }
        }
    }

    /// 息屏、减弱动态效果或离开前台时，无动画地直接回到静止终态。
    // 息屏、减弱动态效果或离开前台时，无动画地直接回到静止终态
    private func stopMotion() {
        // 新建事务，用于关闭隐式动画
        var transaction = Transaction()
        // 关闭隐式动画
        transaction.disablesAnimations = true
        // 在事务里设为已进场并熄灭光效
        withTransaction(transaction) { entered = true; updateGlow = false }
        // 光效版本加一
        glowRevision += 1
    }

    // Token 页：总量卡片与同步状态
    private var tokensPage: some View {
        // 可滚动容器
        ScrollView {
            // 纵向排列，左对齐
            VStack(alignment: .leading, spacing: 0) {
                // 有快照：显示总量或不可用/未同步提示
                if let display = model.manifest?.snapshot {
                    // 纵向排列，左对齐
                    VStack(alignment: .leading, spacing: 9) {
                        // 标题标签
                        Label("Token 总量", systemImage: "number")
                            // 字体与强调色
                            .font(.caption.weight(.medium)).foregroundStyle(accent)
                        // 有用量数据
                        if let tokens = display.tokens {
                            // Token 总量
                            tokenTotal(tokens.all)
                            // 日期文字
                            Text(tokens.day.date).font(.caption2).foregroundStyle(.secondary)
                        // 否则分支
                        } else {
                            // Token 暂不可用或尚未同步的提示
                            Text(display.tokenError != nil ? "Token 暂不可用" : "Token 尚未同步")
                                // 次要字体与颜色
                                .font(.caption).foregroundStyle(.secondary)
                            // 引导文字
                            Text("在 iPhone 中连接桌面后自动同步")
                                // 次要字体与颜色
                                .font(.caption2).foregroundStyle(.secondary)
                                // 允许多行
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    // 卡片外观，更新时高亮
                    .watchPanel(panel, highlighted: updateGlow && allowsMotion)
                    // 测试标识
                    .accessibilityIdentifier("watch-token-card")
                    // 同步状态
                    SyncStatus(receivedAtMs: model.manifest?.receivedAtMs, message: model.message)
                        // 内边距
                        .padding(.horizontal, 13).padding(.top, 8)
                // 没有快照：等待同步
                } else {
                    // 等待数据的占位面板
                    waitingPanel
                }
            }
            // 水平内边距
            .padding(.horizontal, 4)
            // 底部留白
            .padding(.bottom, 22)
        }
        // 页面读屏标签
        .accessibilityLabel("Token 总量页面")
    }

    // 额度页：有数据显示各账户额度，否则显示等待
    private var quotaPage: some View {
        // 可滚动容器
        ScrollView {
            // 纵向排列，左对齐
            VStack(alignment: .leading, spacing: 10) {
                // 有额度快照时
                if let quota = model.manifest?.snapshot?.providerQuota {
                    // 显示额度内容
                    WatchProviderQuotaContent(quota: quota)
                        // 卡片外观，更新时高亮
                        .watchPanel(panel, highlighted: updateGlow && allowsMotion)
                        // 测试标识
                        .accessibilityIdentifier("watch-quota-card")
                // 否则分支
                } else {
                    // 等待额度的占位卡片
                    VStack(alignment: .leading, spacing: 12) {
                        // 标题
                        Text("等待额度数据").font(.headline)
                        // 引导文字
                        Text("在 iPhone 中连接桌面后自动同步")
                            // 次要字体与颜色
                            .font(.caption).foregroundStyle(.secondary)
                            // 允许多行
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    // 卡片外观
                    .watchPanel(panel)
                    // 测试标识
                    .accessibilityIdentifier("watch-quota-waiting")
                }
            }
            // 水平内边距
            .padding(.horizontal, 4)
            // 底部留白
            .padding(.bottom, 22)
        }
        // 页面读屏标签
        .accessibilityLabel("账户剩余额度页面")
    }

    /// 每屏固定一张活动卡；沿用外层页尺寸，避免Watch分页压缩内层测量高度。
    /// 上下滚动吸附切换，左右手势留给主页面；底部只为分页圆点留空。
    // Hooks 页：每屏固定一张活动卡；沿用外层页尺寸，避免分页压缩内层测量高度。上下滚动吸附切换，左右手势留给主页面；底部只为分页圆点留空
    private func hooksPage(size: CGSize) -> some View {
        // 可用宽度，两侧各留 4
        let width = max(0, size.width - 8)
        // 底部留白
        let bottomInset: CGFloat = 14
        // 可用高度
        let height = max(0, size.height - bottomInset)
        // 纵向滚动
        return ScrollView(.vertical) {
            // 已有展示快照时
            if let display = model.manifest?.snapshot {
                // 不可用
                if !display.hooksAvailable {
                    // Token 不可用时的占位
                    hookPlaceholder("暂不可用")
                // 没有 Hook 时
                } else if display.hooks.isEmpty {
                    // 没有 Hook 时的占位
                    hookPlaceholder("暂无活动")
                // 否则分支
                } else {
                    // 按槽位排序，每张卡占满一屏
                    LazyVStack(spacing: 0) {
                        // 按槽位排序逐个显示 Hook
                        ForEach(display.hooks.sorted { $0.slot < $1.slot }, id: \.slot) { hook in
                            // 点击打开详情
                            Button {
                                // 点击时选中该 Hook
                                selectedHook = hook
                            // 按钮标签
                            } label: {
                                // Hook 卡片
                                hookTile(hook, height: height)
                            }
                                // 朴素按钮样式
                                .buttonStyle(.plain)
                                // 以槽位作标识
                                .id(hook.slot)
                                // 宽高
                                .frame(width: width, height: height)
                                // 合并子元素为一个读屏元素
                                .accessibilityElement(children: .combine)
                                // 读屏文案：代理与状态
                                .accessibilityLabel("\(agentTitle(hook.agent))，\(behaviorTitle(hook.behavior))")
                                // 测试标识
                                .accessibilityIdentifier("watch-hook-\(hook.slot)")
                        }
                    // 让每张卡片成为滚动吸附的目标
                    }.scrollTargetLayout()
                }
            // 没有快照时
            } else {
                // 显示等待面板
                waitingPanel
            }
        }
        // 按活动卡自身对齐；系统安全区会使滚动视口与固定卡高不同，按视口分页会逐页偏移。
        // 按活动卡自身对齐；系统安全区会让滚动视口与固定卡高不同，按视口分页会逐页偏移
        .watchHookPaging()
        // 绑定当前可见槽位
        .scrollPosition(id: $visibleHookSlot)
        // 隐藏滚动条
        .scrollIndicators(.hidden)
        // 滚动内容边距
        .contentMargins(.vertical, 0, for: .scrollContent)
        // 宽高
        .frame(width: width, height: height)
        // 裁剪
        .clipped()
        // 水平内边距
        .padding(.horizontal, 4)
        // 底部留白
        .padding(.bottom, bottomInset)
        // 页面读屏标签
        .accessibilityLabel("活动状态页面")
    }

    // 等待 iPhone 同步的提示卡
    private var waitingPanel: some View {
        // 纵向排列
        VStack(alignment: .leading, spacing: 12) {
            // 手机图标
            Image(systemName: "iphone").font(.title2).foregroundStyle(accent)
            // 标题
            Text("等待 iPhone 同步").font(.headline)
            // 引导文字
            Text("在 iPhone 中连接桌面后自动同步")
                // 次要字体与颜色
                .font(.caption).foregroundStyle(.secondary)
                // 允许多行
                .fixedSize(horizontal: false, vertical: true)
            // 有附加消息时
            if let message = model.message {
                // 附加提示
                Text(message).font(.caption2).foregroundStyle(.secondary)
                    // 允许多行
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        // 卡片外观
        .watchPanel(panel)
        // 测试标识
        .accessibilityIdentifier("watch-waiting")
    }

    /// Hooks 不可用或为空时的占位卡片。
    // Hooks 不可用或为空时的占位卡片
    private func hookPlaceholder(_ title: String) -> some View {
        // 纵向排列
        VStack(alignment: .leading, spacing: 8) {
            // 标题
            Text(title).font(.caption)
            // 引导文字
            Text("在 iPhone 中连接桌面后自动同步").font(.caption2)
                // 允许多行
                .fixedSize(horizontal: false, vertical: true)
        }
        // 次要颜色
        .foregroundStyle(.secondary)
        // 占满宽度并套面板外观
        .frame(maxWidth: .infinity, alignment: .leading).watchPanel(panel)
    }

    /// 大字便于扫读，小字保留完整整数；两者均来自同一精确值。
    // Token 总量：大字便于扫读，小字保留完整整数；两者来自同一精确值
    private func tokenTotal(_ value: TokenAmount) -> some View {
        // 纵向排列
        VStack(alignment: .leading, spacing: 3) {
            // 紧凑的金额文本
            Text(compactAmountText(value))
                // 等宽大字号
                .font(.system(size: 31, weight: .semibold, design: .monospaced))
                // 等宽数字、单行并允许缩小
                .monospacedDigit().lineLimit(1).minimumScaleFactor(0.25)
                // 左对齐占满
                .frame(maxWidth: .infinity, alignment: .leading)
                // 隐私敏感
                .privacySensitive()
                // 数字变化动画，不允许动画时直接替换
                .contentTransition(allowsMotion ? .numericText() : .identity)
                // 数值变化时的动画
                .animation(allowsMotion ? .easeOut(duration: 0.28) : nil, value: amountText(value))
            // 有数值才显示精确整数
            if value.total != nil {
                // 完整金额文本
                let exact = amountText(value)
                // 精确值文字
                Text(exact).font(.system(size: 11, weight: .medium, design: .monospaced))
                    // 等宽数字、弱化颜色
                    .monospacedDigit().foregroundStyle(.white.opacity(0.65))
                    // 单行并允许缩小
                    .lineLimit(1).minimumScaleFactor(0.5)
                    // 左对齐占满
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        // 合并为一个读屏元素
        .accessibilityElement(children: .ignore)
        // 读屏文案：总 Token 及限定词
        .accessibilityLabel("总 Token，\(amountText(value))\(amountQuality(value).isEmpty ? "" : "，" + amountQuality(value))")
        // 隐私敏感
        .privacySensitive()
    }

    /// 单张活动卡：顶部一行显示 Agent 与状态，其余空间留给图片。
    // 单张活动卡：顶部一行显示代理与状态，其余空间留给图片
    private func hookTile(_ hook: HookItem, height: CGFloat) -> some View {
        // 卡片内边距
        let inset: CGFloat = 9
        // 纵向排列
        return VStack(alignment: .leading, spacing: 6) {
            // 横向排列，底部对齐
            HStack(alignment: .bottom, spacing: 6) {
                // 代理名称
                Text(agentTitle(hook.agent))
                    // 字体
                    .font(.system(size: 12, weight: .semibold))
                    // 单行并允许缩小
                    .lineLimit(1).minimumScaleFactor(0.65)
                    // 左对齐占满
                    .frame(maxWidth: .infinity, alignment: .leading)
                // 状态圆点与文字
                HStack(spacing: 3) {
                    // 行为状态的色点
                    Circle().fill(behaviorColor(hook.behavior)).frame(width: 5, height: 5)
                    // 行为标题
                    Text(behaviorTitle(hook.behavior)).font(.system(size: 11, weight: .semibold))
                        // 单行
                        .lineLimit(1)
                }
                // 行为状态的颜色
                .foregroundStyle(behaviorColor(hook.behavior))
                // 不压缩
                .fixedSize(horizontal: true, vertical: false)
            }
            // 标题行高度随内容
            .fixedSize(horizontal: false, vertical: true)
            // 图片区域
            hookImage(hook)
        }
        // 占满宽度
        .frame(maxWidth: .infinity)
        // 高度扣除内边距
        .frame(height: max(0, height - inset * 2))
        // 卡片外观，更新时高亮
        .watchPanel(panel, highlighted: updateGlow && allowsMotion, padding: inset)
        // 隐私敏感
        .privacySensitive()
        // 状态变化时的过渡
        .animation(allowsMotion ? .easeOut(duration: 0.2) : nil, value: hook.behavior)
    }

    /// 仅当前 Hooks 页的可见卡片播放动图；离页或常亮模式暂停，减弱动态效果时显示首帧。
    // 仅当前 Hooks 页的可见卡片播放动图；离页或常亮模式暂停，减弱动态效果时显示首帧
    private func hookImage(_ hook: HookItem) -> some View {
        // Hook 的图片地址
        let image = model.imageURL(hook.image)
        // 占位文案
        let placeholder = hook.image == nil ? "暂无图片" : "图片未缓存"
        // 可见槽位
        let visibleSlot = visibleHookSlot ?? model.manifest?.snapshot?.hooks.min(by: { $0.slot < $1.slot })?.slot
        // 读取图片区域的实际尺寸，使图片按区域裁剪
        return GeometryReader { area in
            // 分组
            Group {
                // 有图则显示，满足所有条件才允许播放
                if let image {
                    // 图片视图，仅在选中页且无详情时播放
                    WatchHookImage(url: image, playbackEnabled: selectedHook == nil && selectedPage == Page.hooks.rawValue && allowsMotion && hook.slot == visibleSlot)
                // 否则显示占位
                } else {
                    // 无图时的占位，纵向排列
                    VStack(spacing: 4) {
                        // 占位图标，对读屏隐藏
                        Image(systemName: "photo").accessibilityHidden(true)
                        // 占位文字
                        Text(placeholder).font(.caption2)
                    // 次要颜色
                    }.foregroundStyle(.secondary)
                }
            }
            // 尺寸与区域一致
            .frame(width: area.size.width, height: area.size.height)
            // 圆角裁剪
            .clipShape(RoundedRectangle(cornerRadius: 10))
        }
        // 最小高度 44
        .frame(minHeight: 44)
        // 读屏文案
        .accessibilityLabel(image == nil ? placeholder : "活动图片")
    }

}

// 视图扩展
private extension View {
    // 按系统版本选择分页行为
    @ViewBuilder
    // 按系统版本选择按卡片对齐的分页行为
    func watchHookPaging() -> some View {
        // watchOS 11 起一次只翻一张
        if #available(watchOS 11, *) {
            // 较新系统：每次滚动一页
            scrollTargetBehavior(.viewAligned(limitBehavior: .alwaysByOne))
        // 否则分支
        } else {
            // 否则：按对齐规则滚动
            scrollTargetBehavior(.viewAligned(limitBehavior: .always))
        }
    }
}
