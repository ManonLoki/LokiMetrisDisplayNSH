// 引入 SwiftUI
import SwiftUI

// 视图扩展
extension View {
    /// 统一的 Watch 卡片外观；`highlighted` 为 true 时边框短暂高亮提示有更新。
    // 统一的手表卡片外观；highlighted 为 true 时边框短暂高亮，提示有更新
    func watchPanel(_ color: Color, highlighted: Bool = false, padding: CGFloat = 13) -> some View {
        // 套上面板样式修饰器
        modifier(WatchCyberPanel(color: color, highlighted: highlighted, padding: padding))
    }
}

// 卡片外观修饰器
private struct WatchCyberPanel: ViewModifier {
    // 常亮低亮度模式下去掉装饰
    @Environment(\.isLuminanceReduced) private var reducedLuminance
    // 底色
    let color: Color
    // 是否高亮
    let highlighted: Bool
    // 内边距
    let padding: CGFloat
    // 青色强调色
    private let cyan = Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255)
    // 紫色强调色
    private let violet = Color(red: 174 / 255, green: 140 / 255, blue: 255 / 255)

    // 给内容套上面板样式
    func body(content: Content) -> some View {
        // 内边距
        content.padding(padding)
            // 占满宽度并左对齐
            .frame(maxWidth: .infinity, alignment: .leading)
            // 圆角渐变底
            .background {
                // 圆角矩形背景
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    // 渐变填充
                    .fill(LinearGradient(colors: [color, Color(red: 5 / 255, green: 10 / 255, blue: 18 / 255)], startPoint: .topLeading, endPoint: .bottomTrailing))
                    // 电路网格，低亮度时不画
                    .overlay {
                        // 亮度未降低时
                        if !reducedLuminance {
                            // 叠加电路网格
                            WatchCircuitGrid().stroke(cyan.opacity(0.055), lineWidth: 0.5)
                                // 圆角裁剪
                                .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                        }
                    }
                    // 渐变描边，高亮或低亮度时改变强度
                    .overlay {
                        // 圆角矩形边框
                        RoundedRectangle(cornerRadius: 16, style: .continuous)
                            // 渐变描边
                            .strokeBorder(LinearGradient(colors: [cyan.opacity(reducedLuminance ? 0.12 : (highlighted ? 0.85 : 0.38)), .white.opacity(0.05), violet.opacity(reducedLuminance ? 0.08 : 0.3)], startPoint: .topLeading, endPoint: .bottomTrailing), lineWidth: 0.7)
                    }
                    // 左上角的小亮线
                    .overlay(alignment: .topLeading) {
                        // 亮度未降低时
                        if !reducedLuminance {
                            // 顶部高亮条，更新时更亮
                            Capsule().fill(cyan.opacity(highlighted ? 0.9 : 0.55))
                                // 宽 28 高 1 并留左边距
                                .frame(width: 28, height: 1).padding(.leading, 17)
                        }
                    }
                    // 高亮时加一圈青色光晕
                    .shadow(color: cyan.opacity(highlighted && !reducedLuminance ? 0.18 : 0), radius: 8)
                    // 对读屏隐藏
                    .accessibilityHidden(true)
            }
    }
}

/// 静态几何网格无需逐帧绘制或常驻计时器。
// 静态几何网格：不需要逐帧绘制或常驻计时器
private struct WatchCircuitGrid: Shape {
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
