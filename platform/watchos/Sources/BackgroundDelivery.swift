// 引入 Foundation
import Foundation

/// 一次性的 continuation 门;取消可能发生在 continuation 安装之前。
// 一次性的 continuation 闸门：取消可能发生在 continuation 安装之前
final class BackgroundDelivery: @unchecked Sendable {
    // 锁
    private let lock = NSLock()
    // 等待中的 continuation
    private var continuation: CheckedContinuation<Void, Never>?
    // 是否已完成
    private var completed = false

    // 是否已完成
    var isCompleted: Bool {
        // 加锁
        lock.lock()
        // 结束时解锁
        defer { lock.unlock() }
        // 返回是否已完成
        return completed
    }

    // 安装 continuation：若已完成则立即恢复，否则保存等待
    func install(_ value: CheckedContinuation<Void, Never>) {
        // 加锁
        lock.lock()
        // 已完成
        if completed {
            // 解锁
            lock.unlock()
            // 立即恢复
            value.resume()
        // 否则分支
        } else {
            // 保存 continuation
            continuation = value
            // 解锁
            lock.unlock()
        }
    }

    // 标记完成并恢复等待者
    func finish() {
        // 加锁
        lock.lock()
        // 标记已完成
        completed = true
        // 取出 continuation
        let value = continuation
        // 清空
        continuation = nil
        // 解锁
        lock.unlock()
        // 恢复等待者
        value?.resume()
    }
}
