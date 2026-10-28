// 引入 Foundation
import Foundation

// 仅在有 UIKit 的平台编译（测试可在其他平台编译下面的执行器）
#if canImport(UIKit)
// 引入后台任务、UIKit 与 WidgetKit
import BackgroundTasks
// 界面库
import UIKit
// 小组件库
import WidgetKit

/// Rust 所有者只会收到系统授权的共享目录。
// 导出给 Rust：返回系统授予的共享目录路径
@_cdecl("loki_widget_host_directory")
// 导出：返回共享目录路径
public func lokiWidgetHostDirectory() -> UnsafeMutablePointer<CChar>? {
    // 尝试获取
    do {
        // 共享容器目录
        let directory = try WidgetContainer.directory()
        // 复制为 C 字符串，失败则记录内存不足
        guard let result = directory.path.withCString({ strdup($0) }) else {
            // 记录目录不可用
            WidgetHostDirectoryDiagnostic.record(available: false,
                // 内存不足的错误
                error: NSError(domain: NSPOSIXErrorDomain, code: Int(ENOMEM)))
            // 返回 nil
            return nil
        }
        // 记录目录可用
        WidgetHostDirectoryDiagnostic.record(available: true)
        // 返回结果
        return result
    // 目录不可用：记录并返回空指针
    } catch {
        // 记录目录不可用及错误
        WidgetHostDirectoryDiagnostic.record(available: false, error: error)
        // 返回 nil
        return nil
    }
}

/// 只有有限的状态和稳定的错误码会写入本应用的缓存。
/// 诊断信息是可选的,绝不决定共享存储是否可用。
// 目录查询的诊断：只把有界的状态和稳定的错误码写进本应用的缓存；诊断可有可无，不决定共享存储是否可用
private enum WidgetHostDirectoryDiagnostic {
    // 记录目录诊断
    static func record(available: Bool, error: Error? = nil) {
        // 尝试写入
        do {
            // 取缓存目录
            guard let cache = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first else { return }
            // 文件管理器
            let manager = FileManager.default
            // 诊断目录路径
            let directory = cache.appendingPathComponent("WidgetHost", isDirectory: true)
            // 目录不存在
            if !manager.fileExists(atPath: directory.path) {
                // 创建诊断目录
                try manager.createDirectory(at: directory, withIntermediateDirectories: true,
                    // 首次解锁后才可访问
                    attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
            }
            // 目录必须是真实目录
            let attributes = try directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            // 须是目录且不是符号链接
            guard attributes.isDirectory == true, attributes.isSymbolicLink != true else { return }
            // 诊断文件路径
            let file = directory.appendingPathComponent("directory.json", isDirectory: false)
            // 已有文件必须是普通文件
            if manager.fileExists(atPath: file.path) {
                // 读取文件属性
                let attributes = try file.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
                // 须是普通文件且不是符号链接
                guard attributes.isRegularFile == true, attributes.isSymbolicLink != true else { return }
            }
            // 记录可用性与时间
            var fields: [String: Any] = ["available": available, "atMs": Date().epochMs]
            // 错误域只保留短小的 ASCII 标识，否则记为 unknown
            if let error = error as NSError? {
                // 错误域
                let domain = error.domain
                // 错误域非空、不超过 96 字节且字符合规
                let stableDomain = !domain.isEmpty && domain.utf8.count <= 96 && domain.utf8.allSatisfy {
                    // 只允许字母数字
                    (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) ||
                    // 允许字母数字及 - . _
                    $0 == 45 || $0 == 46 || $0 == 95
                }
                // 域不稳定时记为 unknown
                fields["errorDomain"] = stableDomain ? domain : "unknown"
                // 记录错误码
                fields["errorCode"] = error.code
            }
            // 序列化诊断字段（键排序）
            let data = try JSONSerialization.data(withJSONObject: fields, options: [.sortedKeys])
            // 诊断不得超过 1 KiB
            guard data.count <= 1024 else { return }
            // 原子写入并施加文件保护
            try data.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        // 共享目录的查询与这份可选缓存互不依赖
        } catch {
            // 共享目录的查找独立于这个可选缓存,始终能够成功。
        }
    }
}

/// 释放由 loki_widget_host_directory 返回的目录字符串。
// 释放 loki_widget_host_directory 返回的字符串
@_cdecl("loki_widget_host_free")
// 导出：释放字符串
public func lokiWidgetHostFree(_ value: UnsafeMutablePointer<CChar>?) {
    // 释放内存
    free(value)
}

/// Rust 所有者会先合并内容变更,再请求系统重新加载。
// Rust 所有者合并内容变化后，请求系统刷新小组件
@_cdecl("loki_widget_host_reload")
// 导出：重载小组件
public func lokiWidgetHostReload() {
    // 在主线程执行
    DispatchQueue.main.async {
        // 重载该类型的时间线
        WidgetCenter.shared.reloadTimelines(ofKind: WidgetContainer.kind)
    }
}

/// 由可执行文件构造器调用,原生模块也会再次安全地调用它。
// 由可执行文件的构造函数调用，原生模块也会再安全地调用一次
@_cdecl("loki_widget_host_bootstrap")
// 导出：启动引导
public func lokiWidgetHostBootstrap() {
    // 在主线程注册
    if Thread.isMainThread {
        // 注册生命周期
        WidgetHostLifecycle.shared.register()
    // 否则切到主线程
    } else {
        // 否则切到主线程注册
        DispatchQueue.main.async { WidgetHostLifecycle.shared.register() }
    }
}

/// 这个宿主垫片只负责系统生命周期和一次有限的 HTTP 请求。
// 这个宿主垫片里只有系统生命周期和一次有界的 HTTP 请求
private final class WidgetHostLifecycle {
    // 单例
    static let shared = WidgetHostLifecycle()
    // 后台任务标识
    static let taskIdentifier = "com.manonloki.lokimetrisdisplay.widget-refresh"
    // 是否已注册
    private var registered = false
    // 通知观察者
    private var observers: [NSObjectProtocol] = []
    // 正在运行的后台刷新
    private var running: (id: UUID, execution: WidgetBackgroundExecution)?

    // 注册后台任务并监听启动与进入后台，以安排下一次刷新
    func register() {
        // 只注册一次
        guard !registered else { return }
        // 注册后台任务
        registered = BGTaskScheduler.shared.register(
            // 任务标识与主队列
            forTaskWithIdentifier: Self.taskIdentifier, using: .main
        // 任务回调
        ) { [weak self] task in
            // 不是应用刷新任务则直接标记失败
            guard let self, let refresh = task as? BGAppRefreshTask else {
                // 标记任务失败完成
                task.setTaskCompleted(success: false)
                // 返回
                return
            }
            // 先安排下一次
            self.schedule()
            // 取消仍在运行的旧刷新
            self.running?.execution.cancel()
            // 生成执行标识
            let id = UUID()
            // 创建后台执行
            let execution = WidgetBackgroundExecution(operation: {
                // 执行刷新
                let success = await WidgetContainer.refresh(origin: "background")
                // 成功后让系统重载时间线
                if success, !Task.isCancelled {
                    // 回到主线程
                    await MainActor.run {
                        // 重载小组件时间线
                        WidgetCenter.shared.reloadTimelines(ofKind: WidgetContainer.kind)
                    }
                }
                // 成功且未被取消
                return success && !Task.isCancelled
            // 完成后上报系统，并清理运行状态
            }, completion: { [weak self] success in
                // 清除到期处理
                refresh.expirationHandler = nil
                // 标记任务完成
                refresh.setTaskCompleted(success: success)
                // 回到主队列
                DispatchQueue.main.async {
                    // 仍是当前任务才清除
                    guard self?.running?.id == id else { return }
                    // 清除运行中的任务
                    self?.running = nil
                }
            })
            // 系统要求到期时取消
            refresh.expirationHandler = { [weak execution] in execution?.cancel() }
            // 记录运行中的任务
            self.running = (id, execution)
            // 启动执行
            execution.start()
        }
        // 已注册才继续
        guard registered else { return }
        // 通知中心
        let center = NotificationCenter.default
        // 启动与进入后台时安排刷新
        for name in [UIApplication.didFinishLaunchingNotification, UIApplication.didEnterBackgroundNotification] {
            // 登记观察者
            observers.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                // 重新安排下一次刷新
                self?.schedule()
            })
        }
    }

    // 安排下一次应用刷新：后台刷新可用才提交
    private func schedule() {
        // 已注册且后台刷新可用才继续
        guard registered, UIApplication.shared.backgroundRefreshStatus == .available else { return }
        // 创建后台刷新请求
        let request = BGAppRefreshTaskRequest(identifier: Self.taskIdentifier)
        // 最早在一个刷新间隔之后
        request.earliestBeginDate = Date(timeIntervalSinceNow: WidgetContainer.refreshInterval)
        // 提交刷新请求，失败忽略
        try? BGTaskScheduler.shared.submit(request)
    }
}

#endif

/// 在报告完成之前先取消正在进行的工作,包括过期竞态的情况。
/// 仅依赖 Foundation,以便直接测试操作系统适配器的生命周期保证。
// 先取消正在运行的工作，再上报完成，包括到期竞态；只依赖 Foundation，便于直接测试系统适配器的生命周期保证
final class WidgetBackgroundExecution: @unchecked Sendable {
    // 要执行的异步操作
    private let operation: () async -> Bool
    // 完成回调
    private let completion: (Bool) -> Void
    // 锁
    private let lock = NSLock()
    // 工作任务
    private var work: Task<Void, Never>?
    // 是否已启动
    private var started = false
    // 是否已取消
    private var cancelled = false
    // 是否已完成
    private var completed = false

    // 保存操作与完成回调
    init(operation: @escaping () async -> Bool, completion: @escaping (Bool) -> Void) {
        // 操作
        self.operation = operation
        // 完成回调
        self.completion = completion
    }

    // 启动：只能启动一次，已完成则忽略
    func start() {
        // 加锁
        lock.lock()
        // 已启动或已完成则解锁并返回
        guard !started, !completed else { lock.unlock(); return }
        // 标记已启动
        started = true
        // 创建工作任务
        work = Task {
            // 执行操作
            let success = await operation()
            // 以结果结束
            finish(success: success)
        }
        // 解锁后再回调
        lock.unlock()
    }

    // 取消：未启动就立即以失败完成；已启动则取消任务，由操作清理后返回
    func cancel() {
        // 加锁
        lock.lock()
        // 已完成则解锁并返回
        guard !completed else { lock.unlock(); return }
        // 标记已取消
        cancelled = true
        // 取出工作任务
        let pending = work
        // 未启动则可立即结束
        let finishImmediately = !started
        // 立即结束时标记已完成
        if finishImmediately { completed = true }
        // 解锁
        lock.unlock()
        // 未启动就被取消
        if finishImmediately {
            // 立即以失败完成
            completion(false)
        // 已启动则取消任务
        } else {
            // 操作必须在其取消处理器完成清理之后才能返回。
            // 操作必须在其取消处理器清理完毕之后才返回
            pending?.cancel()
        }
    }

    // 完成：只有第一次有效；被取消过的一律视为失败
    private func finish(success: Bool) {
        // 加锁
        lock.lock()
        // 已完成则解锁并返回
        guard !completed else { lock.unlock(); return }
        // 标记已完成
        completed = true
        // 清空工作任务
        work = nil
        // 成功且未被取消才算成功
        let result = success && !cancelled
        // 解锁
        lock.unlock()
        // 回调结果
        completion(result)
    }
}
