// 引入 SwiftUI
import SwiftUI

// 手表应用入口
@main
// 手表应用入口
struct LokiMetrisWatchApp: App {
    // 界面模型
    @StateObject private var model: WatchModel
    // 场景阶段
    @Environment(\.scenePhase) private var phase

    // 初始化
    init() {
        // 即使 watchOS 只是为了投递数据而启动我们,也会提前初始化进程。
        // 提前初始化进程级运行时；watchOS 仅为投递而启动应用时也会执行
        let runtime = WatchRuntime.shared
        // 创建状态模型
        _model = StateObject(wrappedValue: WatchModel(runtime: runtime))
    }

    // 应用场景
    var body: some Scene {
        // 单窗口
        WindowGroup {
            // 内容视图绑定模型
            ContentView(model: model)
                // 强调色
                .tint(Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255))
                // 回到前台时刷新
                .onChange(of: phase) { _, phase in
                    // 回到活动状态时通知前台
                    if phase == .active { model.foreground() }
                }
        }
        // 系统因 WatchConnectivity 唤醒时处理后台投递
        .backgroundTask(.watchConnectivity) {
            // 处理后台投递
            await WatchRuntime.shared.backgroundDelivery()
        }
    }
}
