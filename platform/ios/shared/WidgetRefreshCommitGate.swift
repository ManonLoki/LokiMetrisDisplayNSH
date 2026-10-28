// 引入 Foundation
import Foundation

/// 取消操作与有限的同步存储提交共用同一个排序点。
// 取消与有界的同步存储提交共用一个排序点：取消之后不能再提交
final class WidgetRefreshCommitGate: @unchecked Sendable {
    // 锁
    private let lock = NSLock()
    // 是否已取消
    private var cancelled = false

    // 标记取消
    func cancel() {
        // 加锁
        lock.lock()
        // 标记已取消
        cancelled = true
        // 解锁
        lock.unlock()
    }

    // 在锁内执行提交；已取消则抛出取消错误
    func commit<T>(_ body: () throws -> T) throws -> T {
        // 加锁
        lock.lock()
        // 结束时解锁
        defer { lock.unlock() }
        // 已取消则抛出取消错误
        guard !cancelled else { throw CancellationError() }
        // 执行提交
        return try body()
    }
}
