// 灵动岛活动与锁屏横幅的系统展示。
import SwiftUI
import WidgetKit

#if canImport(ActivityKit)
// 引入 ActivityKit 与 SwiftUI
import ActivityKit

/// 灵动岛活动的 WidgetKit 入口：ActivityConfiguration 提供锁屏与灵动岛两套视图。
// 灵动岛活动声明
struct AgentActivityWidget: Widget {
    // 锁屏与灵动岛布局
    var body: some WidgetConfiguration {
        // 以共享属性绑定活动状态
        return ActivityConfiguration(for: AgentActivityAttributes.self) { context in
            // 锁屏横幅
            AgentActivityLockView(state: context.state, sourceName: context.attributes.sourceName)
                // 深色底
                .activityBackgroundTint(Color(red: 0.08, green: 0.09, blue: 0.11))
        } dynamicIsland: { context in
            // 灵动岛分区
            DynamicIsland {
                // 展开区：Agent 与行为
                DynamicIslandExpandedRegion(.leading) {
                    // Agent 展示名
                    Text(context.state.agent)
                        // 加粗
                        .font(.headline)
                        // 前景白
                        .foregroundStyle(.white)
                }
                // 展开区右侧：行为标签
                DynamicIslandExpandedRegion(.trailing) {
                    // 行为标签
                    Text(context.state.behavior)
                        // 次要字号
                        .font(.subheadline)
                        // 浅灰
                        .foregroundStyle(.secondary)
                }
                // 底部：状态文案
                DynamicIslandExpandedRegion(.bottom) {
                    // 状态文案
                    Text(context.state.content)
                        // 小字
                        .font(.caption)
                        // 最多两行
                        .lineLimit(2)
                }
            } compactLeading: {
                // 紧凑左侧：Agent 展示名
                Text(context.state.agent)
                    // 小字加粗
                    .font(.caption2).bold()
            } compactTrailing: {
                // 紧凑右侧：行为标签
                Text(context.state.behavior)
                    // 小字
                    .font(.caption2)
            } minimal: {
                // 最小：Agent 首字
                Text(String(context.state.agent.prefix(1)))
                    // 小字加粗
                    .font(.caption2).bold()
            }
        }
    }
}

/// 锁屏横幅视图：Agent、行为与状态文案。
// 锁屏横幅
struct AgentActivityLockView: View {
    // 活动状态
    let state: AgentActivityAttributes.ContentState
    // 来源显示名
    let sourceName: String
    // 锁屏横幅布局
    var body: some View {
        // 横向布局
        HStack(spacing: 10) {
            // 左列：来源与 Agent
            VStack(alignment: .leading, spacing: 2) {
                // 来源名
                Text(sourceName)
                    // 小字
                    .font(.caption2)
                    // 浅灰
                    .foregroundStyle(.secondary)
                // Agent 展示名
                Text(state.agent)
                    // 标题
                    .font(.headline)
                    // 前景白
                    .foregroundStyle(.white)
            }
            // 弹性间距
            Spacer()
            // 右列：行为与文案
            VStack(alignment: .trailing, spacing: 2) {
                // 行为标签
                Text(state.behavior)
                    // 次要字号
                    .font(.subheadline)
                    // 浅灰
                    .foregroundStyle(.secondary)
                // 状态文案
                Text(state.content)
                    // 小字
                    .font(.caption)
                    // 最多两行
                    .lineLimit(2)
                    // 多行截断
                    .multilineTextAlignment(.trailing)
            }
        }
        // 内边距
        .padding(12)
    }
}
#endif
