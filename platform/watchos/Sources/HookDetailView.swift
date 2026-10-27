import SwiftUI

struct HookDetailView: View {
    let hook: HookItem
    @ObservedObject var model: WatchModel
    @Environment(\.dismiss) private var dismiss
    private let panel = Color(red: 17 / 255, green: 29 / 255, blue: 45 / 255)
    var body: some View {
        ScrollView {
            Group {
                if let current = model.manifest?.snapshot?.hooks.first(where: { $0.slot == hook.slot }) {
                    details(current)
                } else {
                    Text("活动已更新，请返回查看最新状态")
                        .font(.caption).foregroundStyle(.secondary)
                        .watchPanel(panel)
                }
            }
            .padding(.horizontal, 4)
        }
        .background(Color.black)
        .navigationTitle(agentTitle(model.manifest?.snapshot?.hooks.first(where: { $0.slot == hook.slot })?.agent ?? hook.agent))
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("关闭", systemImage: "xmark") { dismiss() }
            }
        }
        .accessibilityIdentifier("watch-hook-detail")
    }

    private func details(_ hook: HookItem) -> some View {
        VStack(alignment: .leading, spacing: 12) {
                Label(behaviorTitle(hook.behavior), systemImage: "circle.fill")
                    .font(.caption).foregroundStyle(behaviorColor(hook.behavior))
                if let url = model.imageURL(hook.image) {
                    WatchHookImage(url: url, playbackEnabled: true)
                        .frame(height: 150)
                        .clipShape(RoundedRectangle(cornerRadius: 10))
                        .accessibilityIdentifier("watch-hook-image")
                } else if hook.image != nil {
                    Label("图片未缓存", systemImage: "photo")
                        .font(.caption).foregroundStyle(.secondary)
                }
                if !hook.content.isEmpty { Text(hook.content).font(.body) }
                if hook.contentTruncated { Text("内容已截断").font(.caption2).foregroundStyle(.secondary) }
        }
        .watchPanel(panel)
    }
}
