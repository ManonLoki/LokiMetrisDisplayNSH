import SwiftUI

/// Hooks、Token、额度从左到右独立分页；首次显示Token，以语义ID记住上次页面。
@MainActor
struct ContentView: View {
    private enum Page: String, Hashable, CaseIterable { case hooks, tokens, quota }
    private let panel = Color(red: 17 / 255, green: 29 / 255, blue: 45 / 255)
    private let accent = Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255)
    @Environment(\.scenePhase) private var phase
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.isLuminanceReduced) private var reducedLuminance
    @ObservedObject var model: WatchModel
    @State private var entered = false
    @State private var updateGlow = false
    @State private var glowRevision = 0
    @State private var visibleHookSlot: Int?
    @State private var selectedHook: HookItem?
    @AppStorage("watch.panel.selectedPage") private var selectedPage = Page.tokens.rawValue

    private var allowsMotion: Bool {
        phase == .active && !reduceMotion && !reducedLuminance
    }

    var body: some View {
        dashboard
            .sheet(item: $selectedHook) { hook in
                NavigationStack { HookDetailView(hook: hook, model: model) }
            }
    }

    private var dashboard: some View {
        GeometryReader { pageArea in
            TabView(selection: $selectedPage) {
                hooksPage(size: pageArea.size).tag(Page.hooks.rawValue)
                tokensPage.tag(Page.tokens.rawValue)
                quotaPage.tag(Page.quota.rawValue)
            }
            .frame(width: pageArea.size.width, height: pageArea.size.height)
            .tabViewStyle(.page(indexDisplayMode: .never))
            .accessibilityIdentifier("watch-pages")
            .overlay(alignment: .bottom) {
                HStack(spacing: 5) {
                    ForEach(Page.allCases, id: \.rawValue) { page in
                        Circle().fill(selectedPage == page.rawValue ? .white : .white.opacity(0.35))
                            .frame(width: 4, height: 4)
                    }
                }
                .frame(height: 4)
                .padding(.bottom, 5)
                .allowsHitTesting(false)
                .accessibilityHidden(true)
            }
        }
        .ignoresSafeArea(.container, edges: .bottom)
        .opacity(entered || !allowsMotion ? 1 : 0.5)
        .scaleEffect(entered || !allowsMotion ? 1 : 0.98, anchor: .top)
        .background(Color.black)
        .tint(accent)
        .onAppear {
            if Page(rawValue: selectedPage) == nil { selectedPage = Page.tokens.rawValue }
        }
        .onChange(of: model.manifest?.snapshot?.hooks.map(\.slot), initial: true) { _, slots in
            guard let slots, !slots.isEmpty else { visibleHookSlot = nil; return }
            // 初始位置交给滚动容器报告，避免恢复 Hooks 页时程序定位覆盖系统安全区。
            if let slot = visibleHookSlot, !slots.contains(slot) { visibleHookSlot = nil }
        }
        .onChange(of: phase, initial: true) { _, value in
            if value == .active {
                if allowsMotion {
                    glowRevision += 1
                } else { stopMotion() }
            } else { stopMotion() }
        }
        .onChange(of: model.manifest?.snapshot) { old, new in
            if old != new, allowsMotion { glowRevision += 1 }
        }
        .onChange(of: reduceMotion) { _, _ in if !allowsMotion { stopMotion() } }
        .onChange(of: reducedLuminance) { _, _ in if !allowsMotion { stopMotion() } }
        .task(id: phase) {
            guard allowsMotion else { return }
            var transaction = Transaction()
            transaction.disablesAnimations = true
            withTransaction(transaction) { entered = false }
            do { try await Task.sleep(for: .milliseconds(35)) }
            catch { return }
            guard allowsMotion else { return }
            withAnimation(.easeOut(duration: 0.3)) { entered = true }
        }
        .task(id: glowRevision) {
            guard allowsMotion else { return }
            withAnimation(.easeOut(duration: 0.2)) { updateGlow = true }
            do { try await Task.sleep(for: .milliseconds(450)) }
            catch { return }
            guard allowsMotion else { return }
            withAnimation(.easeOut(duration: 0.35)) { updateGlow = false }
        }
    }

    /// 息屏、减弱动态效果或离开前台时，无动画地直接回到静止终态。
    private func stopMotion() {
        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) { entered = true; updateGlow = false }
        glowRevision += 1
    }

    private var tokensPage: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                if let display = model.manifest?.snapshot {
                    VStack(alignment: .leading, spacing: 9) {
                        Label("Token 总量", systemImage: "number")
                            .font(.caption.weight(.medium)).foregroundStyle(accent)
                        if let tokens = display.tokens {
                            tokenTotal(tokens.all)
                            Text(tokens.day.date).font(.caption2).foregroundStyle(.secondary)
                        } else {
                            Text(display.tokenError != nil ? "Token 暂不可用" : "Token 尚未同步")
                                .font(.caption).foregroundStyle(.secondary)
                            Text("在 iPhone 中连接桌面后自动同步")
                                .font(.caption2).foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    .watchPanel(panel, highlighted: updateGlow && allowsMotion)
                    .accessibilityIdentifier("watch-token-card")
                    SyncStatus(receivedAtMs: model.manifest?.receivedAtMs, message: model.message)
                        .padding(.horizontal, 13).padding(.top, 8)
                } else {
                    waitingPanel
                }
            }
            .padding(.horizontal, 4)
            .padding(.bottom, 22)
        }
        .accessibilityLabel("Token 总量页面")
    }

    private var quotaPage: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                if let quota = model.manifest?.snapshot?.providerQuota {
                    WatchProviderQuotaContent(quota: quota)
                        .watchPanel(panel, highlighted: updateGlow && allowsMotion)
                        .accessibilityIdentifier("watch-quota-card")
                } else {
                    VStack(alignment: .leading, spacing: 12) {
                        Text("等待额度数据").font(.headline)
                        Text("在 iPhone 中连接桌面后自动同步")
                            .font(.caption).foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .watchPanel(panel)
                    .accessibilityIdentifier("watch-quota-waiting")
                }
            }
            .padding(.horizontal, 4)
            .padding(.bottom, 22)
        }
        .accessibilityLabel("账户剩余额度页面")
    }

    /// 每屏固定一张活动卡；沿用外层页尺寸，避免Watch分页压缩内层测量高度。
    /// 上下滚动吸附切换，左右手势留给主页面；底部只为分页圆点留空。
    private func hooksPage(size: CGSize) -> some View {
        let width = max(0, size.width - 8)
        let bottomInset: CGFloat = 14
        let height = max(0, size.height - bottomInset)
        return ScrollView(.vertical) {
            if let display = model.manifest?.snapshot {
                if !display.hooksAvailable {
                    hookPlaceholder("暂不可用")
                } else if display.hooks.isEmpty {
                    hookPlaceholder("暂无活动")
                } else {
                    LazyVStack(spacing: 0) {
                        ForEach(display.hooks.sorted { $0.slot < $1.slot }, id: \.slot) { hook in
                            Button {
                                selectedHook = hook
                            } label: {
                                hookTile(hook, height: height)
                            }
                                .buttonStyle(.plain)
                                .id(hook.slot)
                                .frame(width: width, height: height)
                                .accessibilityElement(children: .combine)
                                .accessibilityLabel("\(agentTitle(hook.agent))，\(behaviorTitle(hook.behavior))")
                                .accessibilityIdentifier("watch-hook-\(hook.slot)")
                        }
                    }.scrollTargetLayout()
                }
            } else {
                waitingPanel
            }
        }
        // 按活动卡自身对齐；系统安全区会使滚动视口与固定卡高不同，按视口分页会逐页偏移。
        .watchHookPaging()
        .scrollPosition(id: $visibleHookSlot)
        .scrollIndicators(.hidden)
        .contentMargins(.vertical, 0, for: .scrollContent)
        .frame(width: width, height: height)
        .clipped()
        .padding(.horizontal, 4)
        .padding(.bottom, bottomInset)
        .accessibilityLabel("活动状态页面")
    }

    private var waitingPanel: some View {
        VStack(alignment: .leading, spacing: 12) {
            Image(systemName: "iphone").font(.title2).foregroundStyle(accent)
            Text("等待 iPhone 同步").font(.headline)
            Text("在 iPhone 中连接桌面后自动同步")
                .font(.caption).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let message = model.message {
                Text(message).font(.caption2).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .watchPanel(panel)
        .accessibilityIdentifier("watch-waiting")
    }

    /// Hooks 不可用或为空时的占位卡片。
    private func hookPlaceholder(_ title: String) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(.caption)
            Text("在 iPhone 中连接桌面后自动同步").font(.caption2)
                .fixedSize(horizontal: false, vertical: true)
        }
        .foregroundStyle(.secondary)
        .frame(maxWidth: .infinity, alignment: .leading).watchPanel(panel)
    }

    /// 大字便于扫读，小字保留完整整数；两者均来自同一精确值。
    private func tokenTotal(_ value: TokenAmount) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(compactAmountText(value))
                .font(.system(size: 31, weight: .semibold, design: .monospaced))
                .monospacedDigit().lineLimit(1).minimumScaleFactor(0.25)
                .frame(maxWidth: .infinity, alignment: .leading)
                .privacySensitive()
                .contentTransition(allowsMotion ? .numericText() : .identity)
                .animation(allowsMotion ? .easeOut(duration: 0.28) : nil, value: amountText(value))
            if value.total != nil {
                let exact = amountText(value)
                Text(exact).font(.system(size: 11, weight: .medium, design: .monospaced))
                    .monospacedDigit().foregroundStyle(.white.opacity(0.65))
                    .lineLimit(1).minimumScaleFactor(0.5)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("总 Token，\(amountText(value))\(amountQuality(value).isEmpty ? "" : "，" + amountQuality(value))")
        .privacySensitive()
    }

    /// 单张活动卡：顶部一行显示 Agent 与状态，其余空间留给图片。
    private func hookTile(_ hook: HookItem, height: CGFloat) -> some View {
        let inset: CGFloat = 9
        return VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .bottom, spacing: 6) {
                Text(agentTitle(hook.agent))
                    .font(.system(size: 12, weight: .semibold))
                    .lineLimit(1).minimumScaleFactor(0.65)
                    .frame(maxWidth: .infinity, alignment: .leading)
                HStack(spacing: 3) {
                    Circle().fill(behaviorColor(hook.behavior)).frame(width: 5, height: 5)
                    Text(behaviorTitle(hook.behavior)).font(.system(size: 11, weight: .semibold))
                        .lineLimit(1)
                }
                .foregroundStyle(behaviorColor(hook.behavior))
                .fixedSize(horizontal: true, vertical: false)
            }
            .fixedSize(horizontal: false, vertical: true)
            hookImage(hook)
        }
        .frame(maxWidth: .infinity)
        .frame(height: max(0, height - inset * 2))
        .watchPanel(panel, highlighted: updateGlow && allowsMotion, padding: inset)
        .privacySensitive()
        .animation(allowsMotion ? .easeOut(duration: 0.2) : nil, value: hook.behavior)
    }

    /// 仅当前 Hooks 页的可见卡片播放动图；离页或常亮模式暂停，减弱动态效果时显示首帧。
    private func hookImage(_ hook: HookItem) -> some View {
        let image = model.imageURL(hook.image)
        let placeholder = hook.image == nil ? "暂无图片" : "图片未缓存"
        let visibleSlot = visibleHookSlot ?? model.manifest?.snapshot?.hooks.min(by: { $0.slot < $1.slot })?.slot
        return GeometryReader { area in
            Group {
                if let image {
                    WatchHookImage(url: image, playbackEnabled: selectedHook == nil && selectedPage == Page.hooks.rawValue && allowsMotion && hook.slot == visibleSlot)
                } else {
                    VStack(spacing: 4) {
                        Image(systemName: "photo").accessibilityHidden(true)
                        Text(placeholder).font(.caption2)
                    }.foregroundStyle(.secondary)
                }
            }
            .frame(width: area.size.width, height: area.size.height)
            .clipShape(RoundedRectangle(cornerRadius: 10))
        }
        .frame(minHeight: 44)
        .accessibilityLabel(image == nil ? placeholder : "活动图片")
    }

}

private extension View {
    @ViewBuilder
    func watchHookPaging() -> some View {
        if #available(watchOS 11, *) {
            scrollTargetBehavior(.viewAligned(limitBehavior: .alwaysByOne))
        } else {
            scrollTargetBehavior(.viewAligned(limitBehavior: .always))
        }
    }
}
