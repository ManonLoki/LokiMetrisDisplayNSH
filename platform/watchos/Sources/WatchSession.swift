import Foundation
import WatchConnectivity

/// Process-lived system transport. The UI never waits for the phone or queries a desktop.
final class WatchSession: NSObject, WCSessionDelegate, @unchecked Sendable {
    private let cache: SnapshotCache
    private let queue = DispatchQueue(label: "com.manonloki.lokimetris.watch-cache")
    private let didChange: @Sendable (String?) -> Void
    private var contentObservation: NSKeyValueObservation?
    // These two fields are accessed only on queue, after queued cache writes.
    private var activationHasCompleted = false
    private var backgroundDeliveries: [UUID: BackgroundDelivery] = [:]

    init(cache: SnapshotCache, didChange: @escaping @Sendable (String?) -> Void) {
        self.cache = cache
        self.didChange = didChange
        super.init()
    }

    func start() {
        guard WCSession.isSupported() else { return }
        let session = WCSession.default
        session.delegate = self
        contentObservation = session.observe(\.hasContentPending, options: [.initial, .new]) { [weak self] _, _ in
            self?.queue.async { [weak self] in self?.finishReadyDeliveries() }
        }
        session.activate()
    }

    func requestLatest() {
        let session = WCSession.default
        guard session.activationState == .activated, session.isReachable else { return }
        session.sendMessage(["op": "requestLatest"], replyHandler: nil, errorHandler: { _ in })
    }

    func session(_ session: WCSession, activationDidCompleteWith activationState: WCSessionActivationState, error: Error?) {
        if let data = session.receivedApplicationContext["lokiCompanion"] as? Data { acceptContext(data) }
        queue.async { [self] in
            activationHasCompleted = true
            finishReadyDeliveries()
        }
        if activationState == .activated { requestLatest() }
    }

    func sessionReachabilityDidChange(_ session: WCSession) {
        if session.isReachable { requestLatest() }
    }

    func session(_ session: WCSession, didReceiveApplicationContext applicationContext: [String: Any]) {
        guard let data = applicationContext["lokiCompanion"] as? Data else { return }
        acceptContext(data)
    }

    func session(_ session: WCSession, didReceive file: WCSessionFile) {
        let metadata = file.metadata ?? [:]
        guard let kind = metadata["kind"] as? String,
              let streamId = metadata["streamId"] as? String,
              let revision = metadata["revision"] as? String,
              ["asset", "snapshot"].contains(kind) else { return }
        let hash = metadata["hash"] as? String
        do {
            // WCSession removes its temporary file after this callback returns.
            let staged = try cache.stageIncoming(file.fileURL)
            queue.async { [self] in
                defer {
                    try? FileManager.default.removeItem(at: staged)
                    finishReadyDeliveries()
                }
                do {
                    if kind == "snapshot" {
                        let size = try staged.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
                        guard size <= 48 * 1024 else { throw CacheError.invalidManifest }
                        let data = try Data(contentsOf: staged)
                        let manifest = try JSONDecoder().decode(CompanionManifest.self, from: data)
                        guard manifest.streamId == streamId, manifest.revision == revision else { throw CacheError.invalidManifest }
                        _ = try cache.receiveManifest(data, origin: .file)
                    } else {
                        guard let hash else { throw CacheError.invalidAsset }
                        _ = try cache.receiveAsset(staged, streamId: streamId, revision: revision, hash: hash)
                    }
                    didChange(nil)
                } catch CacheError.stale, CacheError.unknownStream {
                    // Delayed files cannot change the active source or revive a cleared view.
                } catch { didChange(error.localizedDescription) }
            }
        } catch { didChange(error.localizedDescription) }
    }

    /// SwiftUI keeps the background task alive until WC has no pending content and this queue drained.
    func finishBackgroundDelivery() async {
        let id = UUID()
        let delivery = BackgroundDelivery()
        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                delivery.install(continuation)
                queue.async { [self] in
                    guard !delivery.isCompleted else { return }
                    backgroundDeliveries[id] = delivery
                    // A system cancellation or missing activation cannot leave this task running forever.
                    DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + 20) { [weak self] in
                        delivery.finish()
                        self?.queue.async { [weak self] in
                            self?.backgroundDeliveries.removeValue(forKey: id)?.finish()
                        }
                    }
                    finishReadyDeliveries()
                }
            }
        } onCancel: {
            delivery.finish()
            self.queue.async { [weak self] in self?.backgroundDeliveries.removeValue(forKey: id)?.finish() }
        }
    }

    private func finishReadyDeliveries() {
        let session = WCSession.default
        guard activationHasCompleted,
              session.activationState != .activated || !session.hasContentPending else { return }
        let deliveries = backgroundDeliveries.values
        backgroundDeliveries.removeAll()
        for delivery in deliveries { delivery.finish() }
    }

    private func acceptContext(_ data: Data) {
        queue.async { [self] in
            defer { finishReadyDeliveries() }
            do {
                _ = try cache.receiveManifest(data, origin: .context)
                didChange(nil)
            } catch CacheError.stale, CacheError.unknownStream {
                // Expected when the system delivers an obsolete queued generation.
            } catch { didChange(error.localizedDescription) }
        }
    }
}
