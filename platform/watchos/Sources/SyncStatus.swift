// 引入 SwiftUI
import SwiftUI

/// 保留手机真实接收时间；离线缓存不会被标成实时数据。
// 同步状态：保留手机真实的接收时间；离线缓存不会被标成实时数据
struct SyncStatus: View {
    // 手机接收快照的时间
    let receivedAtMs: UInt64?
    // 附加提示
    let message: String?

    // 视图主体
    var body: some View {
        // 每分钟刷新一次，用来判断数据是否过期
        TimelineView(.periodic(from: .now, by: 60)) { context in
            // 纵向排列
            VStack(alignment: .leading, spacing: 4) {
                // 有接收时间
                if let receivedAtMs {
                    // 毫秒转日期
                    let date = Date(timeIntervalSince1970: Double(receivedAtMs) / 1000)
                    // 显示“本地缓存 · 月日时分”，读屏读完整的接收时间
                    Text("本地缓存 · \(date.formatted(.dateTime.month().day().hour().minute()))")
                        // 读屏文案：接收时间
                        .accessibilityLabel("iPhone 接收时间，\(date.formatted())")
                    // 超过 5 分钟就提示在等待手机更新
                    if context.date.timeIntervalSince(date) > 300 {
                        // 尚未同步的提示
                        Text("等待 iPhone 更新")
                    }
                }
                // 有附加消息则显示
                if let message { Text(message) }
            }
            // 小字号
            .font(.caption2)
            // 次要颜色
            .foregroundStyle(.secondary)
            // 允许多行显示
            .fixedSize(horizontal: false, vertical: true)
        }
    }
}
