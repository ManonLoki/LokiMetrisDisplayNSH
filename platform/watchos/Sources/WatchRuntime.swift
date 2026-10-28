// 引入 Foundation
import Foundation

// 通知名扩展
extension Notification.Name {
    // 缓存变化的通知名
    static let watchCacheChanged = Notification.Name("com.manonloki.lokimetris.watchCacheChanged")
}

/// 由 App.init 创建,包括仅后台启动的情形;从不需要视图或 StateObject。
// 由 App.init 创建，包括仅为后台投递而启动的情形；从不依赖视图或 StateObject
final class WatchRuntime: @unchecked Sendable {
    // 进程级单例
    static let shared = WatchRuntime()
    // 快照缓存
    let cache: SnapshotCache?
    // 与手机的传输会话
    let transport: WatchSession?
    // 启动失败时给用户的提示
    let startupMessage: String?

    // 私有初始化（单例）
    private init() {
        // 尝试创建缓存
        do {
            // 得到缓存及是否禁用传输
            let (cache, disableTransport) = try Self.makeCache()
            // 保存缓存
            self.cache = cache
            // 无启动提示
            self.startupMessage = nil
            // 缓存变化时发通知，附带可能的错误文案
            self.transport = disableTransport ? nil : WatchSession(cache: cache) { message in
                // 发出缓存变化通知
                NotificationCenter.default.post(name: .watchCacheChanged, object: nil,
                                                // 附带错误信息
                                                userInfo: message.map { ["error": $0] })
            }
            // 启动传输
            transport?.start()
        // 缓存无法打开：不创建传输，只给出提示
        } catch {
            // 清空缓存
            cache = nil
            // 清除传输
            transport = nil
            // 记录无法读取缓存的启动提示
            startupMessage = "无法读取本地缓存：\(error.localizedDescription)"
        }
    }

    // 后台投递：交给传输层等待当前投递完成
    func backgroundDelivery() async {
        // 结束后台投递
        await transport?.finishBackgroundDelivery()
    }

    // 在应用支持目录下创建缓存；调试构建可用环境变量隔离验证数据并关闭传输
    private static func makeCache() throws -> (SnapshotCache, Bool) {
        // 应用支持目录
        let directory = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                                   // 要求创建目录
                                                   appropriateFor: nil, create: true)
        // 缓存目录
        var cacheDirectory = directory.appendingPathComponent("companion-cache-v1")
        #if DEBUG
        let environment = ProcessInfo.processInfo.environment
        // 验证用的 ID 必须是规范 UUID
        let validation = environment["LOKI_WATCH_VALIDATION_ID"].flatMap { canonicalUUID($0) ? $0 : nil }
        // 校验模式下使用独立缓存目录
        if let validation { cacheDirectory = directory.appendingPathComponent("validation/\(validation)") }
        #endif
        let cache = try SnapshotCache(root: cacheDirectory)
        #if DEBUG
        try loadFixtureIfRequested(cache)
        // 校验模式下禁用传输
        if validation != nil && environment["LOKI_WATCH_DISABLE_TRANSPORT"] == "1" { return (cache, true) }
        #endif
        // 返回缓存，传输可用
        return (cache, false)
    }

    #if DEBUG
    // 调试构建：按环境变量从目录加载清单和图片夹具到缓存
    private static func loadFixtureIfRequested(_ cache: SnapshotCache) throws {
        // 必须是绝对路径
        guard let path = ProcessInfo.processInfo.environment["LOKI_WATCH_FIXTURE_DIR"], path.hasPrefix("/") else { return }
        // 校验目录
        let directory = URL(fileURLWithPath: path, isDirectory: true)
        // 读取清单
        let data = try Data(contentsOf: directory.appendingPathComponent("manifest.json"))
        // 接收清单
        _ = try cache.receiveManifest(data, origin: .context)
        // 逐个资源：存在则复制为临时文件，暂存后交给缓存
        if let current = cache.manifest {
            // 逐个资源
            for asset in current.assets {
                // 源文件路径
                let source = directory.appendingPathComponent("assets/\(asset.hash)")
                // 源文件存在
                if FileManager.default.fileExists(atPath: source.path) {
                    // 临时文件路径
                    let temporary = directory.appendingPathComponent("\(UUID().uuidString).incoming")
                    // 复制到临时文件
                    try FileManager.default.copyItem(at: source, to: temporary)
                    // 暂存
                    let staged = try cache.stageIncoming(temporary)
                    // 接收资源
                    _ = try cache.receiveAsset(staged, streamId: current.streamId, revision: current.revision, hash: asset.hash)
                }
            }
        }
    }
    #endif
}
