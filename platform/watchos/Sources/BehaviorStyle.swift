// 引入 SwiftUI
import SwiftUI

/// 原版手表配色,分页卡片与详情视图共用。
// 手表沿用的原始配色，分页卡片与详情共用：运行青色、询问紫色、空闲半透明白、出错橙色
func behaviorColor(_ kind: String) -> Color {
    // 按行为种类取颜色
    switch kind {
    // 运行中为青色
    case "running": return Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255)
    // 询问为紫色
    case "asking": return Color(red: 174 / 255, green: 140 / 255, blue: 255 / 255)
    // 空闲为半透明白
    case "idle": return .white.opacity(0.7)
    // 其余为橙色
    default: return .orange
    }
}
