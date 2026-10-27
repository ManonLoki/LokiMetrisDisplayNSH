import Foundation

/// A one-shot continuation gate; cancellation may happen before its continuation is installed.
final class BackgroundDelivery: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Void, Never>?
    private var completed = false

    var isCompleted: Bool {
        lock.lock()
        defer { lock.unlock() }
        return completed
    }

    func install(_ value: CheckedContinuation<Void, Never>) {
        lock.lock()
        if completed {
            lock.unlock()
            value.resume()
        } else {
            continuation = value
            lock.unlock()
        }
    }

    func finish() {
        lock.lock()
        completed = true
        let value = continuation
        continuation = nil
        lock.unlock()
        value?.resume()
    }
}
