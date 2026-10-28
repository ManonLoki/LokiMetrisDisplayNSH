// 引入摘要与 Foundation
import CryptoKit
import Foundation

// 缓存测试入口
@main
struct CacheTests {
    // 三个测试用 UUID：两条数据流与一个来源
    static let first = "7b9dd6ed-9be7-4e86-9f07-8f24ae634301"
    static let second = "cefcac5a-199b-47c7-8d3f-89eea59e3214"
    static let source = "39acf859-ccec-4e4e-8d87-52d5473ff623"
    static var passed = 0

    // 测试入口
    static func main() async throws {
        // 临时目录，结束后删除
        let temporary = FileManager.default.temporaryDirectory.appendingPathComponent("watch-cache-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: temporary, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: temporary) }
        // 读取测试图片并算出摘要
        let asset = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))
        let hash = SHA256.hash(data: asset).map { String(format: "%02x", $0) }.joined()
        let root = temporary.appendingPathComponent("cache")
        let cache = try SnapshotCache(root: root)
        let revision1 = try manifest(revision: "1", hash: hash, length: asset.count)
        // 空缓存不会捏造快照
        check(cache.manifest == nil, "empty install does not fabricate a snapshot")
        // 文件来源不能授权第一条数据流
        expectFailure("file cannot authorize the first stream") { try cache.receiveManifest(revision1, origin: .file) }
        // 图片比清单先到：被保留，但不改变显示
        let pending = try stage(asset, cache: cache, temporary: temporary)
        try check(try cache.receiveAsset(pending, streamId: first, revision: "1", hash: hash) == false,
              "early image is retained without changing the display")
        // 应用上下文提交第一份清单
        try check(try cache.receiveManifest(revision1, origin: .context), "context commits first manifest")
        // 并安装先到的图片
        check(cache.imageURL(hash: hash) != nil, "context verifies and installs the early image")
        // UInt64 的十进制数字保持精确
        check(cache.manifest?.snapshot?.tokens?.all.total == "18446744073709551615", "UInt64 decimal digits remain exact")
        // 相同修订号是幂等的
        try check(try !cache.receiveManifest(revision1, origin: .context), "equal revision is idempotent")
        // 重新打开：冷启动能恢复本地快照和图片
        let rebuilt = try SnapshotCache(root: root)
        check(rebuilt.manifest == cache.manifest && rebuilt.imageURL(hash: hash) != nil, "cold start restores local snapshot and image")
        // 相同修订号但内容不同：被拒绝
        let conflict = try manifest(revision: "1", hash: hash, length: asset.count, content: "different")
        expectFailure("same revision different snapshot rejected") { try cache.receiveManifest(conflict, origin: .context) }
        // 前进到修订 2
        let revision2 = try manifest(revision: "2", hash: hash, length: asset.count)
        _ = try cache.receiveManifest(revision2, origin: .context)
        // 旧上下文不能回滚内容
        expectFailure("old context cannot roll back content") { try cache.receiveManifest(revision1, origin: .context) }
        // 更旧修订的图片可以补全仍被引用的不可变资源
        let olderImage = try stage(asset, cache: cache, temporary: temporary)
        try check(try cache.receiveAsset(olderImage, streamId: first, revision: "1", hash: hash),
              "older image may complete a still-referenced immutable asset")
        // 清空：持久地移除本地展示和资源
        let cleared = try clear(revision: "3")
        _ = try cache.receiveManifest(cleared, origin: .context)
        check(cache.manifest?.isClear == true && cache.imageURL(hash: hash) == nil, "clear durably removes local display and resources")
        // 旧快照不能让清空复活
        expectFailure("old snapshot cannot revive a clear") { try cache.receiveManifest(revision2, origin: .file) }
        // 迟到的图片也不能让清空复活
        let lateImage = try stage(asset, cache: cache, temporary: temporary)
        expectFailure("late image cannot revive a clear") { try cache.receiveAsset(lateImage, streamId: first, revision: "2", hash: hash) }
        // 清空状态在冷启动后仍然保持
        let afterClear = try SnapshotCache(root: root)
        check(afterClear.manifest?.isClear == true, "clear survives cold start")
        // 切换到新的数据流
        let newStream = try manifest(revision: "1", hash: hash, length: asset.count, stream: second)
        // 文件不能切换数据流，只有应用上下文可以
        expectFailure("file cannot switch a stream") { try cache.receiveManifest(newStream, origin: .file) }
        _ = try cache.receiveManifest(newStream, origin: .context)
        // 已退役的流不能切回
        expectFailure("retired context cannot switch back") { try cache.receiveManifest(cleared, origin: .context) }
        // 退役栅栏在冷启动后仍然有效
        let recovered = try SnapshotCache(root: root)
        expectFailure("retired stream fence survives cold start") { try recovered.receiveManifest(revision1, origin: .context) }
        // 摘要不符的图片被拒绝
        var wrongDigest = asset; wrongDigest.append(0)
        let tampered = try stage(wrongDigest, cache: cache, temporary: temporary)
        expectFailure("image digest mismatch rejected") { try cache.receiveAsset(tampered, streamId: second, revision: "1", hash: hash) }
        // 构造非法清单：非规范修订号、溢出修订号、残缺的清空
        var malformed = try JSONSerialization.jsonObject(with: newStream) as! [String: Any]
        malformed["revision"] = "01"
        expectFailure("noncanonical revision rejected") {
            try cache.receiveManifest(JSONSerialization.data(withJSONObject: malformed), origin: .context)
        }
        malformed["revision"] = "18446744073709551616"
        expectFailure("overflow revision rejected") {
            try cache.receiveManifest(JSONSerialization.data(withJSONObject: malformed), origin: .context)
        }
        // 把修订号改成字符串
        malformed["revision"] = "2"
        // 来源 ID 置空
        malformed["sourceId"] = NSNull()
        // 非法清除指令必须被拒绝
        expectFailure("partial clear rejected") {
            // 以上下文来源接收清单
            try cache.receiveManifest(JSONSerialization.data(withJSONObject: malformed), origin: .context)
        }
        // 互操作：读取 Rust 生成的清单、清空文件和资源
        if let index = CommandLine.arguments.firstIndex(of: "--interop"), CommandLine.arguments.count > index + 1 {
            let directory = URL(fileURLWithPath: CommandLine.arguments[index + 1], isDirectory: true)
            let wire = try Data(contentsOf: directory.appendingPathComponent("manifest.json"))
            let interopRoot = temporary.appendingPathComponent("rust-interop")
            let interop = try SnapshotCache(root: interopRoot)
            let manifest = try JSONDecoder().decode(CompanionManifest.self, from: wire)
            // Rust 生成的图片可以先于清单到达
            for asset in manifest.assets {
                let bytes = try Data(contentsOf: directory.appendingPathComponent(asset.hash))
                let staged = try stage(bytes, cache: interop, temporary: temporary)
                try check(try !interop.receiveAsset(staged, streamId: manifest.streamId, revision: manifest.revision, hash: asset.hash),
                          "Rust-produced image may arrive before its manifest")
            }
            _ = try interop.receiveManifest(wire, origin: .context)
            // Swift 能验证 Rust 生成的资源
            for asset in manifest.assets {
                check(interop.imageURL(hash: asset.hash) != nil, "Rust-produced asset verified by Swift")
            }
            // Rust 的 UInt64 最大值在 Swift 解码后不变
            check(interop.manifest?.snapshot?.tokens?.all.total == "18446744073709551615", "Rust UInt64.max survives Swift decoding")
            // Rust 生成的缓存可以冷启动恢复
            let cold = try SnapshotCache(root: interopRoot)
            check(cold.manifest == interop.manifest, "Rust-produced cache cold restore")
            // Rust 生成的清空清单被接受
            _ = try cold.receiveManifest(Data(contentsOf: directory.appendingPathComponent("clear.json")), origin: .context)
            check(cold.manifest?.isClear == true, "Rust-produced clear manifest accepted")
            // 清空之后旧清单被拒绝
            expectFailure("Rust old manifest rejected after clear") { try cold.receiveManifest(wire, origin: .context) }
        }
        // 后台投递闸门：安装 continuation 之前就取消，也要完成
        let preCancelled = BackgroundDelivery()
        preCancelled.finish()
        await withCheckedContinuation { preCancelled.install($0) }
        check(preCancelled.isCompleted, "background cancellation before continuation install completes")
        // 正常完成：重复 finish 也只恢复一次
        let completed = BackgroundDelivery()
        let normalTask = Task { await withCheckedContinuation { completed.install($0) } }
        await Task.yield()
        completed.finish()
        completed.finish()
        await normalTask.value
        check(completed.isCompleted, "background completion resumes once, including duplicate finish")
        // 任务取消不会泄漏 continuation
        let cancelled = BackgroundDelivery()
        let cancelledTask = Task {
            await withTaskCancellationHandler {
                await withCheckedContinuation { cancelled.install($0) }
            } onCancel: { cancelled.finish() }
        }
        cancelledTask.cancel()
        await cancelledTask.value
        // 后台任务被取消时不会遗留 continuation
        check(cancelled.isCompleted, "background task cancellation never leaks a continuation")
        // 输出通过的检查数
        print("Watch cache: \(passed) checks passed")
        // 写出报告，并说明范围：不覆盖 WatchConnectivity 的真机传输
        if CommandLine.arguments.count > 2 {
            let report: [String: Any] = ["result": "passed", "checks": passed,
                "scope": "Foundation/CryptoKit cache, ordering, clears, exact UInt64, cold restore and background continuation cancellation; not WatchConnectivity device transport"]
            try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
                .write(to: URL(fileURLWithPath: CommandLine.arguments[2]))
        }
    }

    // 断言并累计通过数
    static func check(_ condition: @autoclosure () throws -> Bool, _ name: String) rethrows {
        let value = try condition()
        precondition(value, name)
        passed += 1
    }
    // 期望操作失败
    static func expectFailure<T>(_ name: String, _ operation: () throws -> T) {
        do { _ = try operation(); preconditionFailure(name) } catch { passed += 1 }
    }
    // 把数据写成临时文件并交给缓存暂存
    static func stage(_ data: Data, cache: SnapshotCache, temporary: URL) throws -> URL {
        let file = temporary.appendingPathComponent(UUID().uuidString)
        try data.write(to: file)
        return try cache.stageIncoming(file)
    }
    // 构造一份合法的清单
    static func manifest(revision: String, hash: String, length: Int, stream: String = first,
                         content: String = "等待输入") throws -> Data {
        let amount: [String: Any] = ["total": "18446744073709551615", "availability": "available",
                                    "completeness": "complete", "confidence": "exact"]
        return try JSONSerialization.data(withJSONObject: [
            "version": 1, "streamId": stream, "revision": revision, "sourceId": source,
            "receivedAtMs": 1_793_068_200_000 as UInt64,
            "snapshot": ["version": 1, "tokens": ["day": ["date": "2026-10-27", "zone": "Asia/Shanghai"],
                          // 总量与代理明细
                          "all": amount, "agents": [["agent": "Codex", "amount": amount]]],
                         // 无 Token 错误，Hook 可用
                         "tokenError": NSNull(), "hooksAvailable": true,
                         // 一个 Hook：槽位、代理、行为与内容
                         "hooks": [["slot": 1, "agent": "Codex", "behavior": "asking", "content": content,
                                    // 内容未截断，引用图片哈希
                                    "contentTruncated": false, "image": hash]],
                         // 无额度，启用的代理列表
                         "providerQuota": NSNull(), "enabledAgents": ["Codex"]],
            // 资源清单：哈希、类型与长度
            "assets": [["hash": hash, "mime": "image/png", "length": length]]])
    }
    // 构造清空清单
    static func clear(revision: String) throws -> Data {
        try JSONSerialization.data(withJSONObject: ["version": 1, "streamId": first, "revision": revision,
            "sourceId": NSNull(), "receivedAtMs": NSNull(), "snapshot": NSNull(), "assets": []])
    }
}
