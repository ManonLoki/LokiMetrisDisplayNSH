import Foundation

#if canImport(UIKit)
import BackgroundTasks
import UIKit
import WidgetKit

/// The Rust owner receives only the system-authorized shared directory.
@_cdecl("loki_widget_host_directory")
public func lokiWidgetHostDirectory() -> UnsafeMutablePointer<CChar>? {
    do {
        let directory = try WidgetContainer.directory()
        guard let result = directory.path.withCString({ strdup($0) }) else {
            WidgetHostDirectoryDiagnostic.record(available: false,
                error: NSError(domain: NSPOSIXErrorDomain, code: Int(ENOMEM)))
            return nil
        }
        WidgetHostDirectoryDiagnostic.record(available: true)
        return result
    } catch {
        WidgetHostDirectoryDiagnostic.record(available: false, error: error)
        return nil
    }
}

/// Only bounded status and stable error codes are written to this app's cache.
/// Diagnostics are optional and never determine whether the shared store is usable.
private enum WidgetHostDirectoryDiagnostic {
    static func record(available: Bool, error: Error? = nil) {
        do {
            guard let cache = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first else { return }
            let manager = FileManager.default
            let directory = cache.appendingPathComponent("WidgetHost", isDirectory: true)
            if !manager.fileExists(atPath: directory.path) {
                try manager.createDirectory(at: directory, withIntermediateDirectories: true,
                    attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
            }
            let attributes = try directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            guard attributes.isDirectory == true, attributes.isSymbolicLink != true else { return }
            let file = directory.appendingPathComponent("directory.json", isDirectory: false)
            if manager.fileExists(atPath: file.path) {
                let attributes = try file.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
                guard attributes.isRegularFile == true, attributes.isSymbolicLink != true else { return }
            }
            var fields: [String: Any] = ["available": available, "atMs": Date().epochMs]
            if let error = error as NSError? {
                let domain = error.domain
                let stableDomain = !domain.isEmpty && domain.utf8.count <= 96 && domain.utf8.allSatisfy {
                    (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) ||
                    $0 == 45 || $0 == 46 || $0 == 95
                }
                fields["errorDomain"] = stableDomain ? domain : "unknown"
                fields["errorCode"] = error.code
            }
            let data = try JSONSerialization.data(withJSONObject: fields, options: [.sortedKeys])
            guard data.count <= 1024 else { return }
            try data.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        } catch {
            // Shared-directory lookup succeeds independently of this optional cache.
        }
    }
}

/// Releases a directory string returned by loki_widget_host_directory.
@_cdecl("loki_widget_host_free")
public func lokiWidgetHostFree(_ value: UnsafeMutablePointer<CChar>?) {
    free(value)
}

/// The Rust owner coalesces content changes before requesting a system reload.
@_cdecl("loki_widget_host_reload")
public func lokiWidgetHostReload() {
    DispatchQueue.main.async {
        WidgetCenter.shared.reloadTimelines(ofKind: WidgetContainer.kind)
    }
}

/// Called by the executable constructor and again by the native module safely.
@_cdecl("loki_widget_host_bootstrap")
public func lokiWidgetHostBootstrap() {
    if Thread.isMainThread {
        WidgetHostLifecycle.shared.register()
    } else {
        DispatchQueue.main.async { WidgetHostLifecycle.shared.register() }
    }
}

/// Only system lifecycle and one bounded HTTP request live in this host shim.
private final class WidgetHostLifecycle {
    static let shared = WidgetHostLifecycle()
    static let taskIdentifier = "com.manonloki.lokimetrisdisplay.widget-refresh"
    private var registered = false
    private var observers: [NSObjectProtocol] = []
    private var running: (id: UUID, execution: WidgetBackgroundExecution)?

    func register() {
        guard !registered else { return }
        registered = BGTaskScheduler.shared.register(
            forTaskWithIdentifier: Self.taskIdentifier, using: .main
        ) { [weak self] task in
            guard let self, let refresh = task as? BGAppRefreshTask else {
                task.setTaskCompleted(success: false)
                return
            }
            self.schedule()
            self.running?.execution.cancel()
            let id = UUID()
            let execution = WidgetBackgroundExecution(operation: {
                let success = await WidgetContainer.refresh(origin: "background")
                if success, !Task.isCancelled {
                    await MainActor.run {
                        WidgetCenter.shared.reloadTimelines(ofKind: WidgetContainer.kind)
                    }
                }
                return success && !Task.isCancelled
            }, completion: { [weak self] success in
                refresh.expirationHandler = nil
                refresh.setTaskCompleted(success: success)
                DispatchQueue.main.async {
                    guard self?.running?.id == id else { return }
                    self?.running = nil
                }
            })
            refresh.expirationHandler = { [weak execution] in execution?.cancel() }
            self.running = (id, execution)
            execution.start()
        }
        guard registered else { return }
        let center = NotificationCenter.default
        for name in [UIApplication.didFinishLaunchingNotification, UIApplication.didEnterBackgroundNotification] {
            observers.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                self?.schedule()
            })
        }
    }

    private func schedule() {
        guard registered, UIApplication.shared.backgroundRefreshStatus == .available else { return }
        let request = BGAppRefreshTaskRequest(identifier: Self.taskIdentifier)
        request.earliestBeginDate = Date(timeIntervalSinceNow: WidgetContainer.refreshInterval)
        try? BGTaskScheduler.shared.submit(request)
    }
}

#endif

/// Cancels active work before reporting completion, including expiration races.
/// Foundation-only so the OS adapter's lifecycle guarantees can be tested directly.
final class WidgetBackgroundExecution: @unchecked Sendable {
    private let operation: () async -> Bool
    private let completion: (Bool) -> Void
    private let lock = NSLock()
    private var work: Task<Void, Never>?
    private var started = false
    private var cancelled = false
    private var completed = false

    init(operation: @escaping () async -> Bool, completion: @escaping (Bool) -> Void) {
        self.operation = operation
        self.completion = completion
    }

    func start() {
        lock.lock()
        guard !started, !completed else { lock.unlock(); return }
        started = true
        work = Task {
            let success = await operation()
            finish(success: success)
        }
        lock.unlock()
    }

    func cancel() {
        lock.lock()
        guard !completed else { lock.unlock(); return }
        cancelled = true
        let pending = work
        let finishImmediately = !started
        if finishImmediately { completed = true }
        lock.unlock()
        if finishImmediately {
            completion(false)
        } else {
            // The operation must return after its cancellation handler has cleaned up.
            pending?.cancel()
        }
    }

    private func finish(success: Bool) {
        lock.lock()
        guard !completed else { lock.unlock(); return }
        completed = true
        work = nil
        let result = success && !cancelled
        lock.unlock()
        completion(result)
    }
}
