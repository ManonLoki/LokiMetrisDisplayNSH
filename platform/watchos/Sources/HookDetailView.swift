// 引入 SwiftUI
import SwiftUI

// Hook 详情页
struct HookDetailView: View {
    // 进入时的 Hook
    let hook: HookItem
    // 观察模型
    @ObservedObject var model: WatchModel
    // 读取关闭动作
    @Environment(\.dismiss) private var dismiss
    // 面板底色
    private let panel = Color(red: 17 / 255, green: 29 / 255, blue: 45 / 255)
    // 视图主体
    var body: some View {
        // 可滚动容器
        ScrollView {
            // 按是否仍有该 Hook 分组
            Group {
                // 按槽位在最新清单里找到当前 Hook
                if let current = model.manifest?.snapshot?.hooks.first(where: { $0.slot == hook.slot }) {
                    // 显示详情
                    details(current)
                // 找不到说明活动已更新
                } else {
                    // Hook 已不存在时的提示
                    Text("活动已更新，请返回查看最新状态")
                        // 次要字体与颜色
                        .font(.caption).foregroundStyle(.secondary)
                        // 卡片外观
                        .watchPanel(panel)
                }
            }
            // 水平内边距
            .padding(.horizontal, 4)
        }
        // 黑色背景
        .background(Color.black)
        // 标题用最新的代理名，找不到时用进入时的
        .navigationTitle(agentTitle(model.manifest?.snapshot?.hooks.first(where: { $0.slot == hook.slot })?.agent ?? hook.agent))
        // 右上角关闭按钮
        .toolbar {
            // 工具栏项：取消位置
            ToolbarItem(placement: .cancellationAction) {
                // 关闭按钮
                Button("关闭", systemImage: "xmark") { dismiss() }
            }
        }
        // 测试标识
        .accessibilityIdentifier("watch-hook-detail")
    }

    // 详情内容：状态、图片与正文
    private func details(_ hook: HookItem) -> some View {
        // 纵向排列，左对齐
        VStack(alignment: .leading, spacing: 12) {
                // 状态徽标
                Label(behaviorTitle(hook.behavior), systemImage: "circle.fill")
                    // 字体与行为颜色
                    .font(.caption).foregroundStyle(behaviorColor(hook.behavior))
                // 有缓存图片则显示（详情里允许播放动图）
                if let url = model.imageURL(hook.image) {
                    // 图片，开启播放
                    WatchHookImage(url: url, playbackEnabled: true)
                        // 高度 150
                        .frame(height: 150)
                        // 圆角裁剪
                        .clipShape(RoundedRectangle(cornerRadius: 10))
                        // 测试标识
                        .accessibilityIdentifier("watch-hook-image")
                // 有图片但尚未缓存
                } else if hook.image != nil {
                    // 图片未缓存的提示
                    Label("图片未缓存", systemImage: "photo")
                        // 次要字体与颜色
                        .font(.caption).foregroundStyle(.secondary)
                }
                // 正文
                if !hook.content.isEmpty { Text(hook.content).font(.body) }
                // 截断提示
                if hook.contentTruncated { Text("内容已截断").font(.caption2).foregroundStyle(.secondary) }
        }
        // 卡片外观
        .watchPanel(panel)
    }
}
