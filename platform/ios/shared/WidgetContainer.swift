// 引入 Foundation
import Foundation

/// 两个进程从各自Info.plist读取同一组标识；配置缺失时不使用任何替代容器。
/// 摘要、读取凭据与刷新规则都在 Rust 共享存储；这里只定位目录并执行一次网络请求。
// 容器：两个进程各自从 Info.plist 读取同一组标识；配置缺失时不使用任何替代容器。摘要、读取凭据与刷新规则都在 Rust 共享存储，这里只定位目录并执行一次网络请求
enum WidgetContainer {
    // 小组件种类标识
    static let kind = "LokiMetrisDisplay.Summary"
    /// BGAppRefresh 与 Timeline 统一申请 15 分钟目标间隔；实际唤醒仍由系统调度。
    // 后台刷新与 Timeline 统一申请 15 分钟的目标间隔，实际唤醒仍由系统调度
    static let refreshInterval: TimeInterval =
        // 从平台规则读取刷新间隔
        (PlatformRules.dictionary("refresh.limits")?["interval"] as? NSNumber)?.doubleValue ?? 900

    /// 最近一次合法摘要及其展示投影；过期仍返回，并带真实同步时间。
    // 最近一次合法摘要及其展示投影；过期仍会返回，并带真实的同步时间
    struct Entry: Decodable {
        // 接收时间
        let receivedAtMs: Int64
        // 展示视图
        let view: DisplayView
        // 接收日期
        var receivedDate: Date { Date(epochMs: receivedAtMs) }
    }

    /// 只返回系统授予的App Group根目录，不创建或访问其他目录。
    // 只返回系统授予的 App Group 根目录，不创建或访问其他目录
    static func directory() throws -> URL {
        // 读取并校验 App Group 标识，必须以 group. 开头且没有未展开的变量
        guard let group = Bundle.main.object(forInfoDictionaryKey: "LokiWidgetAppGroup") as? String,
              // 共享组须以 group. 开头且不含未展开的变量
              group.hasPrefix("group."), !group.contains("$("),
              // 取共享容器目录
              let directory = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group) else {
            // 共享目录不可用
            throw WidgetSnapshotError.unavailable
        }
        // 目录必须是真实目录而非符号链接
        let attributes = try directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        // 须是目录且不是符号链接
        guard attributes.isDirectory == true, attributes.isSymbolicLink != true else {
            // 共享目录不可用
            throw WidgetSnapshotError.unavailable
        }
        // 返回目录
        return directory
    }

    // 读取缓存的摘要，失败返回 nil
    static func load(atMs now: Int64 = Date().epochMs) -> Entry? {
        // 取共享目录，失败返回 nil
        guard let directory = try? directory() else { return nil }
        // 经展示核心读取条目
        return try? LokiCore.decode(Entry.self, ["op": "store.load", "dir": directory.path, "nowMs": now])
    }

    /// 每个真实执行入口留下有界结果，区分未调度、未授权和实际网络失败。
    /// 诊断写入失败不影响刷新，也不改变缓存中的真实接收时间。
    // 每个真实执行入口留下有界结果，区分未调度、未授权和实际网络失败；诊断写入失败不影响刷新，也不改变缓存里的真实接收时间
    static func refresh(origin: String) async -> Bool {
        // 规范化刷新来源
        let origin = diagnosticOrigin(origin)
        // 尝试开始时间
        let attemptedAtMs = Date().epochMs
        // 先记录“进行中”
        recordRefresh(origin: origin, attemptedAtMs: attemptedAtMs, outcome: "running")
        // 目录不可用：记录错误并返回失败
        guard let directory = try? directory() else {
            // 记录刷新诊断
            recordRefresh(origin: origin, attemptedAtMs: attemptedAtMs, completedAtMs: Date().epochMs, outcome: "error")
            // 返回 false
            return false
        }
        // 执行一次刷新
        let outcome = await WidgetRefresh.refresh(directory: directory)
        // 记录刷新诊断
        recordRefresh(origin: origin, attemptedAtMs: attemptedAtMs, completedAtMs: Date().epochMs, outcome: outcome.rawValue)
        // 成功、已有更新的缓存、仍在租约内，都算成功
        return [.success, .newerCache, .live].contains(outcome)
    }

    // 诊断记录的内容
    private struct RefreshDiagnostic: Encodable {
        // 刷新来源
        let origin: String
        // 尝试开始时间
        let attemptedAtMs: Int64
        // 完成时间
        let completedAtMs: Int64?
        // 结果
        let outcome: String
    }

    /// 调用点名称只允许短 ASCII 标识，既防路径穿越，也不把任意输入写入诊断。
    // 调用点名称只允许短 ASCII 标识，既防路径穿越，也不把任意输入写进诊断
    private static func diagnosticOrigin(_ value: String) -> String {
        // 经平台规则规范化来源
        (try? PlatformRules.call("refresh.diagnosticOrigin", ["value": value])) as? String ?? "unknown"
    }

    /// 固定目录、拒绝符号链接、原子写入；仅保存状态及稳定错误码，不编码网络或凭据信息。
    // 固定目录、拒绝符号链接、原子写入；只保存状态与稳定的错误码，不编码网络或凭据信息
    private static func recordRefresh(origin: String, attemptedAtMs: Int64, completedAtMs: Int64? = nil,
                                      // 结果
                                      outcome: String) {
        // 尝试写入
        do {
            // 构造诊断记录
            let diagnostic = RefreshDiagnostic(origin: origin, attemptedAtMs: attemptedAtMs,
                                               // 完成时间与结果
                                               completedAtMs: completedAtMs, outcome: outcome)
            // 编码诊断记录
            let data = try JSONEncoder().encode(diagnostic)
            // 记录不得超过 1 KiB
            guard data.count <= 1024 else { return }
            // 文件管理器
            let manager = FileManager.default
            // 诊断目录
            var directory = try Self.directory()
            // 读取根目录属性
            let root = try directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            // 根须是目录且不是链接
            guard root.isDirectory == true, root.isSymbolicLink != true else { return }
            // 逐级创建并检查目录：每一级都必须是真实目录
            for component in ["Library", "Caches", "WidgetRefresh"] {
                // 逐级追加路径组件
                directory.appendPathComponent(component, isDirectory: true)
                // 目录不存在
                if !manager.fileExists(atPath: directory.path) {
                    // 创建目录
                    try manager.createDirectory(at: directory, withIntermediateDirectories: false,
                                                // 首次解锁后才可访问
                                                attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
                }
                // 读取目录属性
                let values = try directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
                // 须是目录且不是链接
                guard values.isDirectory == true, values.isSymbolicLink != true else { return }
            }
            // 首次解锁后才可访问
            try manager.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
                                      // 设置文件保护
                                      ofItemAtPath: directory.path)
            // 排除出备份
            var values = URLResourceValues(); values.isExcludedFromBackup = true
            // 设置资源值
            try directory.setResourceValues(values)
            // 诊断文件路径
            let file = directory.appendingPathComponent(origin + ".json", isDirectory: false)
            // 已有文件必须是普通文件而非符号链接
            if manager.fileExists(atPath: file.path) {
                // 读取文件属性
                let values = try file.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
                // 须是普通文件且不是链接
                guard values.isRegularFile == true, values.isSymbolicLink != true else { return }
            }
            // 原子写入，并施加文件保护
            try data.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        // 诊断不可用时仍继续实际刷新，不能让观测文件成为数据更新的前置条件
        } catch {
            // 诊断不可用时仍继续实际刷新，不能让观测文件成为数据更新的前置条件。
        }
    }
}
