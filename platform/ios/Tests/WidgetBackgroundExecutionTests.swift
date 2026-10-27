import Foundation

private actor Signal {
    private var signalled = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    func wait() async {
        if signalled { return }
        await withCheckedContinuation { waiters.append($0) }
    }

    func signal() {
        signalled = true
        let pending = waiters
        waiters = []
        for waiter in pending { waiter.resume() }
    }
}

private final class Observations: @unchecked Sendable {
    private let lock = NSLock()
    private var requests = 0
    private var cleanups = 0
    private var completions: [Bool] = []

    func request() { lock.lock(); requests += 1; lock.unlock() }
    func cleanup() { lock.lock(); cleanups += 1; lock.unlock() }
    func complete(_ success: Bool) { lock.lock(); completions.append(success); lock.unlock() }
    var values: (Int, Int, [Bool]) {
        lock.lock(); defer { lock.unlock() }
        return (requests, cleanups, completions)
    }
}

@main
enum WidgetBackgroundExecutionTests {
    static func main() async {
        await expiresBeforeStart()
        await waitsForCancellationCleanup()
        await completesSuccessfulWorkOnce()
        cancelsBeforeCommit()
        await finishesCommitBeforeCancellation()
        print("Widget background execution: 5 tests passed")
    }

    private static func expiresBeforeStart() async {
        let observed = Observations()
        let execution = WidgetBackgroundExecution(operation: {
            observed.request()
            return true
        }, completion: { observed.complete($0) })
        execution.cancel()
        execution.start()
        execution.cancel()
        let (requests, _, completed) = observed.values
        precondition(requests == 0, "Expired-before-start execution created network work")
        precondition(completed == [false], "Expired-before-start completion was not exactly once")
    }

    private static func waitsForCancellationCleanup() async {
        let started = Signal(), permitCleanup = Signal(), completed = Signal()
        let observed = Observations()
        let execution = WidgetBackgroundExecution(operation: {
            observed.request()
            await started.signal()
            // Cancellation deliberately cannot finish cleanup until the test permits it.
            await permitCleanup.wait()
            observed.cleanup()
            return true
        }, completion: { success in
            precondition(observed.values.1 == 1, "OS completion ran before operation cleanup")
            observed.complete(success)
            Task { await completed.signal() }
        })
        execution.start()
        await started.wait()
        execution.cancel()
        precondition(observed.values.2.isEmpty, "Expiration completed while operation was still active")
        await permitCleanup.signal()
        await completed.wait()
        execution.cancel()
        execution.start()
        let (requests, cleanups, results) = observed.values
        precondition(requests == 1 && cleanups == 1, "Cancellation restarted or skipped operation cleanup")
        precondition(results == [false], "Cancelled work reported success or completed twice")
    }

    private static func completesSuccessfulWorkOnce() async {
        let completed = Signal()
        let observed = Observations()
        let execution = WidgetBackgroundExecution(operation: {
            observed.request()
            return true
        }, completion: { success in
            observed.complete(success)
            Task { await completed.signal() }
        })
        execution.start()
        await completed.wait()
        execution.cancel()
        execution.start()
        let (requests, _, results) = observed.values
        precondition(requests == 1, "Completed execution restarted work")
        precondition(results == [true], "Successful completion was not exactly once")
    }

    private static func cancelsBeforeCommit() {
        let gate = WidgetRefreshCommitGate()
        let observed = Observations()
        gate.cancel()
        do {
            try gate.commit { observed.request() }
            preconditionFailure("Cancelled refresh committed a snapshot")
        } catch is CancellationError {
            precondition(observed.values.0 == 0, "Cancellation lost before the commit gate")
        } catch {
            preconditionFailure("Cancellation produced an unexpected error")
        }
    }

    private static func finishesCommitBeforeCancellation() async {
        let gate = WidgetRefreshCommitGate()
        let entered = Signal(), cancellationAttempted = Signal(), cancelled = Signal()
        let releaseCommit = DispatchSemaphore(value: 0)
        let observed = Observations()
        DispatchQueue.global().async {
            do {
                try gate.commit {
                    Task { await entered.signal() }
                    releaseCommit.wait()
                    observed.cleanup()
                }
            } catch { preconditionFailure("An already-started commit was interrupted") }
        }
        await entered.wait()
        DispatchQueue.global().async {
            Task { await cancellationAttempted.signal() }
            gate.cancel()
            precondition(observed.values.1 == 1, "Cancellation returned before an active commit cleaned up")
            Task { await cancelled.signal() }
        }
        await cancellationAttempted.wait()
        precondition(observed.values.1 == 0, "The controlled commit finished before it was released")
        releaseCommit.signal()
        await cancelled.wait()
        do {
            try gate.commit { observed.request() }
            preconditionFailure("A subsequent commit ignored cancellation")
        } catch is CancellationError {
            precondition(observed.values.0 == 0, "A subsequent cancelled commit performed work")
        } catch { preconditionFailure("Cancellation produced an unexpected error") }
    }
}
