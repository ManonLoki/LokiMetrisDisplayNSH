// 引入 Foundation
import Foundation

// 一次性信号：等待方挂起，信号发出后全部恢复
private actor Signal {
    private var signalled = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    // 已发出则直接返回，否则挂起
    func wait() async {
        if signalled { return }
        await withCheckedContinuation { waiters.append($0) }
    }

    // 发出信号并唤醒所有等待者
    func signal() {
        signalled = true
        let pending = waiters
        waiters = []
        for waiter in pending { waiter.resume() }
    }
}

// 线程安全的观察记录：请求次数、清理次数与完成结果
private final class Observations: @unchecked Sendable {
    private let lock = NSLock()
    private var requests = 0
    private var cleanups = 0
    private var completions: [Bool] = []

    // 记录一次请求
    func request() { lock.lock(); requests += 1; lock.unlock() }
    // 记录一次清理
    func cleanup() { lock.lock(); cleanups += 1; lock.unlock() }
    // 记录一次完成及其结果
    func complete(_ success: Bool) { lock.lock(); completions.append(success); lock.unlock() }
    // 读取三项记录
    var values: (Int, Int, [Bool]) {
        // 加锁读取
        lock.lock(); defer { lock.unlock() }
        // 返回请求次数、清理次数与完成结果
        return (requests, cleanups, completions)
    }
}

// 测试入口
@main
enum WidgetBackgroundExecutionTests {
    // 依次运行五个用例
    static func main() async {
        await expiresBeforeStart()
        await waitsForCancellationCleanup()
        await completesSuccessfulWorkOnce()
        cancelsBeforeCommit()
        await finishesCommitBeforeCancellation()
        print("Widget background execution: 5 tests passed")
    }

    // 启动前就到期：不应发起任何网络工作，并且只以失败完成一次
    private static func expiresBeforeStart() async {
        let observed = Observations()
        let execution = WidgetBackgroundExecution(operation: {
            observed.request()
            return true
        }, completion: { observed.complete($0) })
        // 先取消，再启动，再取消
        execution.cancel()
        execution.start()
        execution.cancel()
        let (requests, _, completed) = observed.values
        precondition(requests == 0, "Expired-before-start execution created network work")
        precondition(completed == [false], "Expired-before-start completion was not exactly once")
    }

    // 取消时要等操作清理完毕才上报完成
    private static func waitsForCancellationCleanup() async {
        let started = Signal(), permitCleanup = Signal(), completed = Signal()
        let observed = Observations()
        let execution = WidgetBackgroundExecution(operation: {
            observed.request()
            await started.signal()
            // 取消操作被有意设计为:在测试放行之前无法完成清理。
            // 取消后，操作要等测试放行才能清理
            await permitCleanup.wait()
            observed.cleanup()
            return true
        }, completion: { success in
            // 系统完成回调必须在操作清理之后
            precondition(observed.values.1 == 1, "OS completion ran before operation cleanup")
            observed.complete(success)
            Task { await completed.signal() }
        })
        execution.start()
        await started.wait()
        // 操作仍在进行时取消
        execution.cancel()
        // 此时还不能已经完成
        precondition(observed.values.2.isEmpty, "Expiration completed while operation was still active")
        // 放行清理
        await permitCleanup.signal()
        await completed.wait()
        // 完成后再取消或启动都不应重复执行
        execution.cancel()
        execution.start()
        let (requests, cleanups, results) = observed.values
        precondition(requests == 1 && cleanups == 1, "Cancellation restarted or skipped operation cleanup")
        // 被取消的工作不报告成功，也不会完成两次
        precondition(results == [false], "Cancelled work reported success or completed twice")
    }

    // 成功的工作只完成一次
    private static func completesSuccessfulWorkOnce() async {
        let completed = Signal()
        let observed = Observations()
        let execution = WidgetBackgroundExecution(operation: {
            observed.request()
            return true
        }, completion: { success in
            observed.complete(success)
            // 通知完成
            Task { await completed.signal() }
        })
        // 启动
        execution.start()
        // 等待完成
        await completed.wait()
        // 完成后再取消、再启动都无效
        execution.cancel()
        execution.start()
        let (requests, _, results) = observed.values
        precondition(requests == 1, "Completed execution restarted work")
        precondition(results == [true], "Successful completion was not exactly once")
    }

    // 取消先于提交：闸门拒绝提交，工作不会执行
    private static func cancelsBeforeCommit() {
        let gate = WidgetRefreshCommitGate()
        let observed = Observations()
        gate.cancel()
        do {
            try gate.commit { observed.request() }
            preconditionFailure("Cancelled refresh committed a snapshot")
        } catch is CancellationError {
            // 取消没有被丢失：工作没有执行
            precondition(observed.values.0 == 0, "Cancellation lost before the commit gate")
        // 出现其他错误
        } catch {
            // 则测试失败
            preconditionFailure("Cancellation produced an unexpected error")
        }
    }

    // 已经开始的提交先完成，取消才返回
    private static func finishesCommitBeforeCancellation() async {
        let gate = WidgetRefreshCommitGate()
        let entered = Signal(), cancellationAttempted = Signal(), cancelled = Signal()
        // 用信号量控制提交何时结束
        let releaseCommit = DispatchSemaphore(value: 0)
        let observed = Observations()
        // 在后台线程开始提交，并停在提交中
        DispatchQueue.global().async {
            do {
                try gate.commit {
                    Task { await entered.signal() }
                    releaseCommit.wait()
                    observed.cleanup()
                }
            } catch { preconditionFailure("An already-started commit was interrupted") }
        }
        // 提交进行中，从另一线程取消
        await entered.wait()
        DispatchQueue.global().async {
            Task { await cancellationAttempted.signal() }
            // 取消必须等进行中的提交完成才返回
            gate.cancel()
            precondition(observed.values.1 == 1, "Cancellation returned before an active commit cleaned up")
            Task { await cancelled.signal() }
        }
        await cancellationAttempted.wait()
        // 此刻受控的提交尚未结束
        precondition(observed.values.1 == 0, "The controlled commit finished before it was released")
        // 放行提交
        releaseCommit.signal()
        await cancelled.wait()
        // 取消之后的新提交必须被拒绝
        do {
            try gate.commit { observed.request() }
            preconditionFailure("A subsequent commit ignored cancellation")
        } catch is CancellationError {
            precondition(observed.values.0 == 0, "A subsequent cancelled commit performed work")
        } catch { preconditionFailure("Cancellation produced an unexpected error") }
    }
}
