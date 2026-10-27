import Foundation

/// Cancellation and the bounded synchronous store commit have one ordering point.
final class WidgetRefreshCommitGate: @unchecked Sendable {
    private let lock = NSLock()
    private var cancelled = false

    func cancel() {
        lock.lock()
        cancelled = true
        lock.unlock()
    }

    func commit<T>(_ body: () throws -> T) throws -> T {
        lock.lock()
        defer { lock.unlock() }
        guard !cancelled else { throw CancellationError() }
        return try body()
    }
}
