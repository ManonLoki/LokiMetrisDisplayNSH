import Foundation

extension Notification.Name {
    static let watchCacheChanged = Notification.Name("com.manonloki.lokimetris.watchCacheChanged")
}

/// Created by App.init, including background-only launches; it never requires a view or StateObject.
final class WatchRuntime: @unchecked Sendable {
    static let shared = WatchRuntime()
    let cache: SnapshotCache?
    let transport: WatchSession?
    let startupMessage: String?

    private init() {
        do {
            let (cache, disableTransport) = try Self.makeCache()
            self.cache = cache
            self.startupMessage = nil
            self.transport = disableTransport ? nil : WatchSession(cache: cache) { message in
                NotificationCenter.default.post(name: .watchCacheChanged, object: nil,
                                                userInfo: message.map { ["error": $0] })
            }
            transport?.start()
        } catch {
            cache = nil
            transport = nil
            startupMessage = "无法读取本地缓存：\(error.localizedDescription)"
        }
    }

    func backgroundDelivery() async {
        await transport?.finishBackgroundDelivery()
    }

    private static func makeCache() throws -> (SnapshotCache, Bool) {
        let directory = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                                   appropriateFor: nil, create: true)
        var cacheDirectory = directory.appendingPathComponent("companion-cache-v1")
        #if DEBUG
        let environment = ProcessInfo.processInfo.environment
        let validation = environment["LOKI_WATCH_VALIDATION_ID"].flatMap { canonicalUUID($0) ? $0 : nil }
        if let validation { cacheDirectory = directory.appendingPathComponent("validation/\(validation)") }
        #endif
        let cache = try SnapshotCache(root: cacheDirectory)
        #if DEBUG
        try loadFixtureIfRequested(cache)
        if validation != nil && environment["LOKI_WATCH_DISABLE_TRANSPORT"] == "1" { return (cache, true) }
        #endif
        return (cache, false)
    }

    #if DEBUG
    private static func loadFixtureIfRequested(_ cache: SnapshotCache) throws {
        guard let path = ProcessInfo.processInfo.environment["LOKI_WATCH_FIXTURE_DIR"], path.hasPrefix("/") else { return }
        let directory = URL(fileURLWithPath: path, isDirectory: true)
        let data = try Data(contentsOf: directory.appendingPathComponent("manifest.json"))
        _ = try cache.receiveManifest(data, origin: .context)
        if let current = cache.manifest {
            for asset in current.assets {
                let source = directory.appendingPathComponent("assets/\(asset.hash)")
                if FileManager.default.fileExists(atPath: source.path) {
                    let temporary = directory.appendingPathComponent("\(UUID().uuidString).incoming")
                    try FileManager.default.copyItem(at: source, to: temporary)
                    let staged = try cache.stageIncoming(temporary)
                    _ = try cache.receiveAsset(staged, streamId: current.streamId, revision: current.revision, hash: asset.hash)
                }
            }
        }
    }
    #endif
}
