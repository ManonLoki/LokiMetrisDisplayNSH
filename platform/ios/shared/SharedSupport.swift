// 引入 Foundation
import Foundation

/// 稳定的失败信息不包含源数据、凭据或文件系统路径。
// 小组件快照错误，只有“无效”和“不可用”两种，不含源数据、凭据或文件路径
enum WidgetSnapshotError: Error { case invalid, unavailable }

/// 固定的 JSON 编解码器,由平台桥接层与 WidgetKit 外壳共用。
// 桥接层与 WidgetKit 外壳共用的固定编码器和解码器
enum SharedJSON {
    // 共享 JSON 编码器
    static let encoder = JSONEncoder()
    // 共享 JSON 解码器
    static let decoder = JSONDecoder()
}
