import SwiftUI

/// 保留手机真实接收时间；离线缓存不会被标成实时数据。
struct SyncStatus: View {
    let receivedAtMs: UInt64?
    let message: String?

    var body: some View {
        TimelineView(.periodic(from: .now, by: 60)) { context in
            VStack(alignment: .leading, spacing: 4) {
                if let receivedAtMs {
                    let date = Date(timeIntervalSince1970: Double(receivedAtMs) / 1000)
                    Text("本地缓存 · \(date.formatted(.dateTime.month().day().hour().minute()))")
                        .accessibilityLabel("iPhone 接收时间，\(date.formatted())")
                    if context.date.timeIntervalSince(date) > 300 {
                        Text("等待 iPhone 更新")
                    }
                }
                if let message { Text(message) }
            }
            .font(.caption2)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
    }
}
