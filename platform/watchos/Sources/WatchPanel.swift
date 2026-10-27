import SwiftUI

extension View {
    /// 统一的 Watch 卡片外观；`highlighted` 为 true 时边框短暂高亮提示有更新。
    func watchPanel(_ color: Color, highlighted: Bool = false, padding: CGFloat = 13) -> some View {
        modifier(WatchCyberPanel(color: color, highlighted: highlighted, padding: padding))
    }
}

private struct WatchCyberPanel: ViewModifier {
    @Environment(\.isLuminanceReduced) private var reducedLuminance
    let color: Color
    let highlighted: Bool
    let padding: CGFloat
    private let cyan = Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255)
    private let violet = Color(red: 174 / 255, green: 140 / 255, blue: 255 / 255)

    func body(content: Content) -> some View {
        content.padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background {
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .fill(LinearGradient(colors: [color, Color(red: 5 / 255, green: 10 / 255, blue: 18 / 255)], startPoint: .topLeading, endPoint: .bottomTrailing))
                    .overlay {
                        if !reducedLuminance {
                            WatchCircuitGrid().stroke(cyan.opacity(0.055), lineWidth: 0.5)
                                .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                        }
                    }
                    .overlay {
                        RoundedRectangle(cornerRadius: 16, style: .continuous)
                            .strokeBorder(LinearGradient(colors: [cyan.opacity(reducedLuminance ? 0.12 : (highlighted ? 0.85 : 0.38)), .white.opacity(0.05), violet.opacity(reducedLuminance ? 0.08 : 0.3)], startPoint: .topLeading, endPoint: .bottomTrailing), lineWidth: 0.7)
                    }
                    .overlay(alignment: .topLeading) {
                        if !reducedLuminance {
                            Capsule().fill(cyan.opacity(highlighted ? 0.9 : 0.55))
                                .frame(width: 28, height: 1).padding(.leading, 17)
                        }
                    }
                    .shadow(color: cyan.opacity(highlighted && !reducedLuminance ? 0.18 : 0), radius: 8)
                    .accessibilityHidden(true)
            }
    }
}

/// 静态几何网格无需逐帧绘制或常驻计时器。
private struct WatchCircuitGrid: Shape {
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
