import SwiftUI

/// WidgetKit仅绘制静态材质；锁屏交还系统单色背景，不运行持续动画。
// 背景材质：WidgetKit 只绘制静态材质；锁屏交还系统的单色背景，不运行持续动画
struct WidgetCyberMaterial: View {
    // 读取小组件尺寸
    @Environment(\.widgetFamily) private var family
    // 青色强调色
    let cyan = Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255)
    // 紫色强调色
    let violet = Color(red: 174 / 255, green: 140 / 255, blue: 255 / 255)

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
struct WidgetCircuitGrid: Shape {
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
