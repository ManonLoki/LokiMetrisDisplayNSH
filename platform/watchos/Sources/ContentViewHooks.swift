import SwiftUI

extension ContentView {
    // 等待 iPhone 同步的提示卡
    var waitingPanel: some View {
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
    func hookPlaceholder(_ title: String) -> some View {
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
    func tokenTotal(_ value: TokenAmount) -> some View {
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
    func hookTile(_ hook: HookItem, height: CGFloat) -> some View {
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
    func hookImage(_ hook: HookItem) -> some View {
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
