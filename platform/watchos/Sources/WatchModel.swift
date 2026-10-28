// 引入 Foundation 与 SwiftUI
import Foundation
// 引入界面库
import SwiftUI

// 界面模型运行在主线程
@MainActor
// 手表界面的数据模型
final class WatchModel: ObservableObject {
    // 当前清单
    @Published private(set) var manifest: CompanionManifest?
    // 附加提示
    @Published private(set) var message: String?
    // 图片版本号，变化时视图重新读取图片
    @Published private(set) var imageRevision = 0
    // 运行时
    private let runtime: WatchRuntime
    // 缓存变化的通知观察者
    private var observer: NSObjectProtocol?

    // 以运行时初始化
    init(runtime: WatchRuntime) {
        // 保存运行时
        self.runtime = runtime
        // 先读一次缓存和启动提示
        manifest = runtime.cache?.manifest
        // 读取启动提示
        message = runtime.startupMessage
        // 缓存变化时在主线程更新清单、提示与图片版本
        observer = NotificationCenter.default.addObserver(forName: .watchCacheChanged, object: nil, queue: .main) { [weak self] notification in
            // 通知里的错误信息
            let message = notification.userInfo?["error"] as? String
            // 回到主线程
            Task { @MainActor [weak self] in
                // 自身已释放则返回
                guard let self else { return }
                // 读取缓存里的清单
                self.manifest = self.runtime.cache?.manifest
                // 更新提示信息
                self.message = message
                // 图片版本加一
                self.imageRevision += 1
            }
        }
        // 在第一次读取与注册观察者之间,可能已经有一次投递提交了。
        // 第一次读取与注册观察者之间可能已有投递提交，所以再读一次
        manifest = runtime.cache?.manifest
    }

    // 销毁时
    deinit {
        // 移除通知观察者
        if let observer { NotificationCenter.default.removeObserver(observer) }
    }

    // 回到前台：重读缓存并向手机请求最新数据
    func foreground() {
        // 刷新清单
        manifest = runtime.cache?.manifest
        // 图片版本加一
        imageRevision += 1
        // 请求最新数据
        runtime.transport?.requestLatest()
    }
    // 取某个图片摘要对应的本地缓存地址
    func imageURL(_ hash: String?) -> URL? {
        // 没有哈希则返回 nil
        guard let hash else { return nil }
        // 取缓存里的图片地址
        return runtime.cache?.imageURL(hash: hash)
    }
}
