import CryptoKit
import Foundation

@main
struct CacheTests {
    static let first = "7b9dd6ed-9be7-4e86-9f07-8f24ae634301"
    static let second = "cefcac5a-199b-47c7-8d3f-89eea59e3214"
    static let source = "39acf859-ccec-4e4e-8d87-52d5473ff623"
    static var passed = 0

    static func main() async throws {
        let temporary = FileManager.default.temporaryDirectory.appendingPathComponent("watch-cache-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: temporary, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: temporary) }
        let asset = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))
        let hash = SHA256.hash(data: asset).map { String(format: "%02x", $0) }.joined()
        let root = temporary.appendingPathComponent("cache")
        let cache = try SnapshotCache(root: root)
        let revision1 = try manifest(revision: "1", hash: hash, length: asset.count)
        check(cache.manifest == nil, "empty install does not fabricate a snapshot")
        expectFailure("file cannot authorize the first stream") { try cache.receiveManifest(revision1, origin: .file) }
        let pending = try stage(asset, cache: cache, temporary: temporary)
        try check(try cache.receiveAsset(pending, streamId: first, revision: "1", hash: hash) == false,
              "early image is retained without changing the display")
        try check(try cache.receiveManifest(revision1, origin: .context), "context commits first manifest")
        check(cache.imageURL(hash: hash) != nil, "context verifies and installs the early image")
        check(cache.manifest?.snapshot?.tokens?.all.total == "18446744073709551615", "UInt64 decimal digits remain exact")
        try check(try !cache.receiveManifest(revision1, origin: .context), "equal revision is idempotent")
        let rebuilt = try SnapshotCache(root: root)
        check(rebuilt.manifest == cache.manifest && rebuilt.imageURL(hash: hash) != nil, "cold start restores local snapshot and image")
        let conflict = try manifest(revision: "1", hash: hash, length: asset.count, content: "different")
        expectFailure("same revision different snapshot rejected") { try cache.receiveManifest(conflict, origin: .context) }
        let revision2 = try manifest(revision: "2", hash: hash, length: asset.count)
        _ = try cache.receiveManifest(revision2, origin: .context)
        expectFailure("old context cannot roll back content") { try cache.receiveManifest(revision1, origin: .context) }
        let olderImage = try stage(asset, cache: cache, temporary: temporary)
        try check(try cache.receiveAsset(olderImage, streamId: first, revision: "1", hash: hash),
              "older image may complete a still-referenced immutable asset")
        let cleared = try clear(revision: "3")
        _ = try cache.receiveManifest(cleared, origin: .context)
        check(cache.manifest?.isClear == true && cache.imageURL(hash: hash) == nil, "clear durably removes local display and resources")
        expectFailure("old snapshot cannot revive a clear") { try cache.receiveManifest(revision2, origin: .file) }
        let lateImage = try stage(asset, cache: cache, temporary: temporary)
        expectFailure("late image cannot revive a clear") { try cache.receiveAsset(lateImage, streamId: first, revision: "2", hash: hash) }
        let afterClear = try SnapshotCache(root: root)
        check(afterClear.manifest?.isClear == true, "clear survives cold start")
        let newStream = try manifest(revision: "1", hash: hash, length: asset.count, stream: second)
        expectFailure("file cannot switch a stream") { try cache.receiveManifest(newStream, origin: .file) }
        _ = try cache.receiveManifest(newStream, origin: .context)
        expectFailure("retired context cannot switch back") { try cache.receiveManifest(cleared, origin: .context) }
        let recovered = try SnapshotCache(root: root)
        expectFailure("retired stream fence survives cold start") { try recovered.receiveManifest(revision1, origin: .context) }
        var wrongDigest = asset; wrongDigest.append(0)
        let tampered = try stage(wrongDigest, cache: cache, temporary: temporary)
        expectFailure("image digest mismatch rejected") { try cache.receiveAsset(tampered, streamId: second, revision: "1", hash: hash) }
        var malformed = try JSONSerialization.jsonObject(with: newStream) as! [String: Any]
        malformed["revision"] = "01"
        expectFailure("noncanonical revision rejected") {
            try cache.receiveManifest(JSONSerialization.data(withJSONObject: malformed), origin: .context)
        }
        malformed["revision"] = "18446744073709551616"
        expectFailure("overflow revision rejected") {
            try cache.receiveManifest(JSONSerialization.data(withJSONObject: malformed), origin: .context)
        }
        malformed["revision"] = "2"
        malformed["sourceId"] = NSNull()
        expectFailure("partial clear rejected") {
            try cache.receiveManifest(JSONSerialization.data(withJSONObject: malformed), origin: .context)
        }
        if let index = CommandLine.arguments.firstIndex(of: "--interop"), CommandLine.arguments.count > index + 1 {
            let directory = URL(fileURLWithPath: CommandLine.arguments[index + 1], isDirectory: true)
            let wire = try Data(contentsOf: directory.appendingPathComponent("manifest.json"))
            let interopRoot = temporary.appendingPathComponent("rust-interop")
            let interop = try SnapshotCache(root: interopRoot)
            let manifest = try JSONDecoder().decode(CompanionManifest.self, from: wire)
            for asset in manifest.assets {
                let bytes = try Data(contentsOf: directory.appendingPathComponent(asset.hash))
                let staged = try stage(bytes, cache: interop, temporary: temporary)
                try check(try !interop.receiveAsset(staged, streamId: manifest.streamId, revision: manifest.revision, hash: asset.hash),
                          "Rust-produced image may arrive before its manifest")
            }
            _ = try interop.receiveManifest(wire, origin: .context)
            for asset in manifest.assets {
                check(interop.imageURL(hash: asset.hash) != nil, "Rust-produced asset verified by Swift")
            }
            check(interop.manifest?.snapshot?.tokens?.all.total == "18446744073709551615", "Rust UInt64.max survives Swift decoding")
            let cold = try SnapshotCache(root: interopRoot)
            check(cold.manifest == interop.manifest, "Rust-produced cache cold restore")
            _ = try cold.receiveManifest(Data(contentsOf: directory.appendingPathComponent("clear.json")), origin: .context)
            check(cold.manifest?.isClear == true, "Rust-produced clear manifest accepted")
            expectFailure("Rust old manifest rejected after clear") { try cold.receiveManifest(wire, origin: .context) }
        }
        let preCancelled = BackgroundDelivery()
        preCancelled.finish()
        await withCheckedContinuation { preCancelled.install($0) }
        check(preCancelled.isCompleted, "background cancellation before continuation install completes")
        let completed = BackgroundDelivery()
        let normalTask = Task { await withCheckedContinuation { completed.install($0) } }
        await Task.yield()
        completed.finish()
        completed.finish()
        await normalTask.value
        check(completed.isCompleted, "background completion resumes once, including duplicate finish")
        let cancelled = BackgroundDelivery()
        let cancelledTask = Task {
            await withTaskCancellationHandler {
                await withCheckedContinuation { cancelled.install($0) }
            } onCancel: { cancelled.finish() }
        }
        cancelledTask.cancel()
        await cancelledTask.value
        check(cancelled.isCompleted, "background task cancellation never leaks a continuation")
        print("Watch cache: \(passed) checks passed")
        if CommandLine.arguments.count > 2 {
            let report: [String: Any] = ["result": "passed", "checks": passed,
                "scope": "Foundation/CryptoKit cache, ordering, clears, exact UInt64, cold restore and background continuation cancellation; not WatchConnectivity device transport"]
            try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
                .write(to: URL(fileURLWithPath: CommandLine.arguments[2]))
        }
    }

    static func check(_ condition: @autoclosure () throws -> Bool, _ name: String) rethrows {
        let value = try condition()
        precondition(value, name)
        passed += 1
    }
    static func expectFailure<T>(_ name: String, _ operation: () throws -> T) {
        do { _ = try operation(); preconditionFailure(name) } catch { passed += 1 }
    }
    static func stage(_ data: Data, cache: SnapshotCache, temporary: URL) throws -> URL {
        let file = temporary.appendingPathComponent(UUID().uuidString)
        try data.write(to: file)
        return try cache.stageIncoming(file)
    }
    static func manifest(revision: String, hash: String, length: Int, stream: String = first,
                         content: String = "等待输入") throws -> Data {
        let amount: [String: Any] = ["total": "18446744073709551615", "availability": "available",
                                    "completeness": "complete", "confidence": "exact"]
        return try JSONSerialization.data(withJSONObject: [
            "version": 1, "streamId": stream, "revision": revision, "sourceId": source,
            "receivedAtMs": 1_793_068_200_000 as UInt64,
            "snapshot": ["version": 1, "tokens": ["day": ["date": "2026-10-27", "zone": "Asia/Shanghai"],
                          "all": amount, "agents": [["agent": "Codex", "amount": amount]]],
                         "tokenError": NSNull(), "hooksAvailable": true,
                         "hooks": [["slot": 1, "agent": "Codex", "behavior": "asking", "content": content,
                                    "contentTruncated": false, "image": hash]],
                         "providerQuota": NSNull(), "enabledAgents": ["Codex"]],
            "assets": [["hash": hash, "mime": "image/png", "length": length]]])
    }
    static func clear(revision: String) throws -> Data {
        try JSONSerialization.data(withJSONObject: ["version": 1, "streamId": first, "revision": revision,
            "sourceId": NSNull(), "receivedAtMs": NSNull(), "snapshot": NSNull(), "assets": []])
    }
}
