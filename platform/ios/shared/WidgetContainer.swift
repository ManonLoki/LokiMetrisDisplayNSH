import Foundation

/// 两个进程从各自Info.plist读取同一组标识；配置缺失时不使用任何替代容器。
/// 摘要、读取凭据与刷新规则都在 Rust 共享存储；这里只定位目录并执行一次网络请求。
enum WidgetContainer {
    static let kind = "LokiMetrisDisplay.Summary"
    /// BGAppRefresh 与 Timeline 统一申请 15 分钟目标间隔；实际唤醒仍由系统调度。
    static let refreshInterval: TimeInterval =
        (PlatformRules.dictionary("refresh.limits")?["interval"] as? NSNumber)?.doubleValue ?? 900

    /// 最近一次合法摘要及其展示投影；过期仍返回，并带真实同步时间。
    struct Entry: Decodable {
        let receivedAtMs: Int64
        let view: DisplayView
        var receivedDate: Date { Date(epochMs: receivedAtMs) }
    }

    /// 只返回系统授予的App Group根目录，不创建或访问其他目录。
    static func directory() throws -> URL {
        guard let group = Bundle.main.object(forInfoDictionaryKey: "LokiWidgetAppGroup") as? String,
              group.hasPrefix("group."), !group.contains("$("),
              let directory = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group) else {
            throw WidgetSnapshotError.unavailable
        }
        let attributes = try directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard attributes.isDirectory == true, attributes.isSymbolicLink != true else {
            throw WidgetSnapshotError.unavailable
        }
        return directory
    }

    static func load(atMs now: Int64 = Date().epochMs) -> Entry? {
        guard let directory = try? directory() else { return nil }
        return try? LokiCore.decode(Entry.self, ["op": "store.load", "dir": directory.path, "nowMs": now])
    }

    /// 每个真实执行入口留下有界结果，区分未调度、未授权和实际网络失败。
    /// 诊断写入失败不影响刷新，也不改变缓存中的真实接收时间。
    static func refresh(origin: String) async -> Bool {
        let origin = diagnosticOrigin(origin)
        let attemptedAtMs = Date().epochMs
        recordRefresh(origin: origin, attemptedAtMs: attemptedAtMs, outcome: "running")
        guard let directory = try? directory() else {
            recordRefresh(origin: origin, attemptedAtMs: attemptedAtMs, completedAtMs: Date().epochMs, outcome: "error")
            return false
        }
        let outcome = await WidgetRefresh.refresh(directory: directory)
        recordRefresh(origin: origin, attemptedAtMs: attemptedAtMs, completedAtMs: Date().epochMs, outcome: outcome.rawValue)
        return [.success, .newerCache, .live].contains(outcome)
    }

    private struct RefreshDiagnostic: Encodable {
        let origin: String
        let attemptedAtMs: Int64
        let completedAtMs: Int64?
        let outcome: String
    }

    /// 调用点名称只允许短 ASCII 标识，既防路径穿越，也不把任意输入写入诊断。
    private static func diagnosticOrigin(_ value: String) -> String {
        (try? PlatformRules.call("refresh.diagnosticOrigin", ["value": value])) as? String ?? "unknown"
    }

    /// 固定目录、拒绝符号链接、原子写入；仅保存状态及稳定错误码，不编码网络或凭据信息。
    private static func recordRefresh(origin: String, attemptedAtMs: Int64, completedAtMs: Int64? = nil,
                                      outcome: String) {
        do {
            let diagnostic = RefreshDiagnostic(origin: origin, attemptedAtMs: attemptedAtMs,
                                               completedAtMs: completedAtMs, outcome: outcome)
            let data = try JSONEncoder().encode(diagnostic)
            guard data.count <= 1024 else { return }
            let manager = FileManager.default
            var directory = try Self.directory()
            let root = try directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            guard root.isDirectory == true, root.isSymbolicLink != true else { return }
            for component in ["Library", "Caches", "WidgetRefresh"] {
                directory.appendPathComponent(component, isDirectory: true)
                if !manager.fileExists(atPath: directory.path) {
                    try manager.createDirectory(at: directory, withIntermediateDirectories: false,
                                                attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
                }
                let values = try directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
                guard values.isDirectory == true, values.isSymbolicLink != true else { return }
            }
            try manager.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
                                      ofItemAtPath: directory.path)
            var values = URLResourceValues(); values.isExcludedFromBackup = true
            try directory.setResourceValues(values)
            let file = directory.appendingPathComponent(origin + ".json", isDirectory: false)
            if manager.fileExists(atPath: file.path) {
                let values = try file.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
                guard values.isRegularFile == true, values.isSymbolicLink != true else { return }
            }
            try data.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        } catch {
            // 诊断不可用时仍继续实际刷新，不能让观测文件成为数据更新的前置条件。
        }
    }
}
