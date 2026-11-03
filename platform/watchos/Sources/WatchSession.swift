// 引入 Foundation 与 WatchConnectivity
import Foundation
// 手表连接库
import WatchConnectivity
// Watch设备名称来自系统公开API，不申请额外名称entitlement。
import WatchKit

/// 与进程同寿命的系统传输层。UI 从不等待手机,也不查询桌面端。
// 随进程存活的系统传输；界面从不等待手机，也不会去查询桌面
final class WatchSession: NSObject, WCSessionDelegate, @unchecked Sendable {
    // 快照缓存
    private let cache: SnapshotCache
    // 串行队列：所有缓存写入和状态变更都在这里
    private let queue = DispatchQueue(label: "com.manonloki.lokimetris.watch-cache")
    // 缓存变化的回调，带可选错误文案
    private let didChange: @Sendable (String?, Data?, String?) -> Void
    // 仅由主线程的真实scenePhase及模型接纳事件推进。
    @MainActor private var foregroundPresence = WatchForegroundPresence()
    private let foregroundGate = WatchForegroundReceiptGate()
    // 观察是否还有待处理内容
    private var contentObservation: NSKeyValueObservation?
    // 这两个字段只在 queue 上访问,且在已排队的缓存写入之后。
    // 以下两个字段只在队列上访问，且在已排队的缓存写入之后
    private var activationHasCompleted = false
    // 等待完成的后台投递
    private var backgroundDeliveries: [UUID: BackgroundDelivery] = [:]

    // 以缓存与变更回调初始化
    init(cache: SnapshotCache, didChange: @escaping @Sendable (String?, Data?, String?) -> Void) {
        // 保存缓存
        self.cache = cache
        // 保存回调
        self.didChange = didChange
        // 调用父类初始化
        super.init()
    }

    // 启动会话：设置代理，观察待处理内容，激活
    func start() {
        // 不支持手表会话则返回
        guard WCSession.isSupported() else { return }
        // 默认会话
        let session = WCSession.default
        // 设置委托
        session.delegate = self
        // 待处理内容变化时，检查是否可以结束后台投递
        contentObservation = session.observe(\.hasContentPending, options: [.initial, .new]) { [weak self] _, _ in
            // 在队列上结束已就绪的投递
            self?.queue.async { [weak self] in self?.finishReadyDeliveries() }
        }
        // 激活会话
        session.activate()
    }

    // 向手机请求最新数据：仅在已激活且可达时
    func requestLatest() {
        // 默认会话
        let session = WCSession.default
        // 已激活且可达才继续
        guard session.activationState == .activated, session.isReachable else { return }
        // 请求最新数据
        session.sendMessage(["op": "requestLatest"], replyHandler: nil, errorHandler: { _ in })
    }

    // 激活完成：先接收已有的应用上下文，标记激活完成，再请求最新数据
    func session(_ session: WCSession, activationDidCompleteWith activationState: WCSessionActivationState, error: Error?) {
        // 读取已收到的上下文
        if let data = session.receivedApplicationContext["lokiCompanion"] as? Data { acceptContext(data, permitsReceipt: false) }
        // 在队列上异步处理
        queue.async { [self] in
            // 标记激活已完成
            activationHasCompleted = true
            // 结束已就绪的投递
            finishReadyDeliveries()
        }
        // 激活后请求最新数据
        if activationState == .activated { requestLatest() }
    }

    // 变为可达时请求最新数据
    func sessionReachabilityDidChange(_ session: WCSession) {
        // 可达时请求最新数据
        if session.isReachable { requestLatest() }
    }

    // 收到新的应用上下文
    func session(_ session: WCSession, didReceiveApplicationContext applicationContext: [String: Any]) {
        // 取出配套上下文
        guard let data = applicationContext["lokiCompanion"] as? Data else { return }
        // 接收上下文
        acceptContext(data)
    }

    // 收到文件：快照清单或图片
    func session(_ session: WCSession, didReceive file: WCSessionFile) {
        // 元数据必须齐全，类型只能是 asset 或 snapshot
        let metadata = file.metadata ?? [:]
        // 元数据里的种类
        guard let kind = metadata["kind"] as? String,
              // 流标识
              let streamId = metadata["streamId"] as? String,
              // 修订号
              let revision = metadata["revision"] as? String,
              // 种类须是资源或快照
              ["asset", "snapshot"].contains(kind) else { return }
        // 文件元数据里的哈希
        let hash = metadata["hash"] as? String
        let receiptForegroundId = foregroundGate.capture()
        // 尝试处理
        do {
            // 此回调返回后,WCSession 会删除它的临时文件。
            // 回调返回后系统会删除临时文件，所以先暂存一份
            let staged = try cache.stageIncoming(file.fileURL)
            // 在队列上异步处理
            queue.async { [self] in
                // 处理完删除暂存文件，并检查是否可结束后台投递
                defer {
                    // 删除暂存文件
                    try? FileManager.default.removeItem(at: staged)
                    // 结束已就绪的投递
                    finishReadyDeliveries()
                }
                // 尝试处理
                do {
                    var adopted: Data?
                    // 清单：不超过 48 KiB，解码后流与修订号必须与元数据一致
                    if kind == "snapshot" {
                        // 文件大小
                        let size = try staged.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
                        // 清单不得超过 48 KiB
                        guard size <= 48 * 1024 else { throw CacheError.invalidManifest }
                        // 读取文件内容
                        let data = try Data(contentsOf: staged)
                        // 解码清单
                        let manifest = try JSONDecoder().decode(CompanionManifest.self, from: data)
                        // 流与修订号须与元数据一致
                        guard manifest.streamId == streamId, manifest.revision == revision else { throw CacheError.invalidManifest }
                        // 作为清单接收
                        if try cache.receiveManifest(data, origin: .file) { adopted = data }
                    // 图片：交给缓存校验后收下
                    } else {
                        // 缺少哈希则抛出
                        guard let hash else { throw CacheError.invalidAsset }
                        // 作为资源接收
                        _ = try cache.receiveAsset(staged, streamId: streamId, revision: revision, hash: hash)
                    }
                    // 通知无错误
                    didChange(nil, adopted, receiptForegroundId)
                // 迟到的文件不能改变当前来源，也不能让已清空的视图复活
                } catch CacheError.stale, CacheError.unknownStream {
                    // 延迟到达的文件不能改变当前来源,也不能让已清除的视图复活。
                } catch { didChange(error.localizedDescription, nil, nil) }
            }
        // 出错则通知错误描述
        } catch { didChange(error.localizedDescription, nil, nil) }
    }

    /// 主线程收到scenePhase事件后立即撤销旧代次；后台缓存和WC投递不改变该事实。
    @MainActor
    func setForeground(_ active: Bool) {
        let revoked = foregroundPresence.setActive(active)
        foregroundGate.set(foregroundPresence.foregroundId)
        guard let old = revoked, let info = presenceInfo(), WCSession.isSupported() else { return }
        let session = WCSession.default
        guard session.activationState == .activated, session.isReachable,
              let data = try? JSONEncoder().encode(WatchPresenceRevocation(deviceId: info.deviceId, foregroundId: old)) else { return }
        // 无论发送成功与否本地资格已经撤销；手机还会通过探针和期限验证，不能借WC状态宣称在线。
        session.sendMessageData(data, replyHandler: nil, errorHandler: { _ in })
    }

    /// WatchModel已经在主线程采用该次changed完整清单后才登记，不把仅写盘或旧缓存重读当同步。
    @MainActor
    func adoptedChanged(_ manifest: CompanionManifest, receiptForegroundId: String?) {
        foregroundPresence.adoptedChanged(manifest, receiptForegroundId: receiptForegroundId, at: ContinuousClock().now)
    }

    /// 只有即时探针得到该回调；replyHandler最后在主线程读取当前代次，排队期间退后台不会返回旧active。
    func session(_ session: WCSession, didReceiveMessageData data: Data, replyHandler: @escaping (Data) -> Void) {
        guard let probe = try? WatchPresenceProbe.decode(data) else {
            replyHandler(Data("{\"version\":1,\"type\":\"presenceError\"}".utf8))
            return
        }
        Task { @MainActor [self] in
            guard let info = presenceInfo(),
                  let response = try? JSONEncoder().encode(foregroundPresence.reply(probe, info: info, at: ContinuousClock().now)) else {
                replyHandler(Data("{\"version\":1,\"type\":\"presenceError\"}".utf8))
                return
            }
            replyHandler(response)
        }
    }

    /// 安装UUID独立存于Watch本地，已有损坏记录失败关闭；公开名称不依赖iPhone。
    @MainActor
    private func presenceInfo() -> WatchPresenceInfo? {
        let defaults = UserDefaults.standard
        let key = "loki.watch.installation-id.v1"
        let deviceId: String
        if let stored = defaults.object(forKey: key) {
            guard let value = stored as? String, canonicalUUID(value) else { return nil }
            deviceId = value
        } else {
            let value = UUID().uuidString.lowercased()
            defaults.set(value, forKey: key)
            guard defaults.string(forKey: key) == value else { return nil }
            deviceId = value
        }
        let rawName = WKInterfaceDevice.current().name.trimmingCharacters(in: .whitespacesAndNewlines)
        let name = !rawName.isEmpty && rawName.utf8.count <= 160
            && rawName.unicodeScalars.allSatisfy({ !CharacterSet.controlCharacters.contains($0) }) ? rawName : "Apple Watch"
        let rawVersion = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "1.0"
        let version = !rawVersion.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && rawVersion.utf8.count <= 64
            && rawVersion.unicodeScalars.allSatisfy({ !CharacterSet.controlCharacters.contains($0) }) ? rawVersion : "1.0"
        return WatchPresenceInfo(deviceId: deviceId, name: name, appVersion: version)
    }

    /// SwiftUI 会让后台任务保持存活,直到 WC 没有待处理内容且此队列已排空。
    // 后台投递：SwiftUI 会让后台任务一直存活，直到 WC 没有待处理内容且这条队列已清空
    func finishBackgroundDelivery() async {
        // 生成投递标识
        let id = UUID()
        // 创建投递
        let delivery = BackgroundDelivery()
        // 支持取消地等待
        await withTaskCancellationHandler {
            // 等待投递完成
            await withCheckedContinuation { continuation in
                // 登记 continuation
                delivery.install(continuation)
                // 在队列上异步处理
                queue.async { [self] in
                    // 已完成则返回
                    guard !delivery.isCompleted else { return }
                    // 登记后台投递
                    backgroundDeliveries[id] = delivery
                    // 系统取消或缺少激活,都不会让此任务永远运行下去。
                    // 系统取消或始终未激活时，任务也不能永远挂着：20 秒后强制结束
                    DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + 20) { [weak self] in
                        // 结束投递
                        delivery.finish()
                        // 在队列上异步处理
                        self?.queue.async { [weak self] in
                            // 移除并结束该投递
                            self?.backgroundDeliveries.removeValue(forKey: id)?.finish()
                        }
                    }
                    // 结束已就绪的投递
                    finishReadyDeliveries()
                }
            }
        // 被取消时立即结束并移除
        } onCancel: {
            // 结束投递
            delivery.finish()
            // 在队列上异步移除该投递
            self.queue.async { [weak self] in self?.backgroundDeliveries.removeValue(forKey: id)?.finish() }
        }
    }

    // 已激活完成，并且会话未激活或没有待处理内容时，结束所有等待中的后台投递
    private func finishReadyDeliveries() {
        // 默认会话
        let session = WCSession.default
        // 激活已完成
        guard activationHasCompleted,
              // 会话未激活，或没有待处理内容
              session.activationState != .activated || !session.hasContentPending else { return }
        // 全部后台投递
        let deliveries = backgroundDeliveries.values
        // 清空投递
        backgroundDeliveries.removeAll()
        // 逐个结束
        for delivery in deliveries { delivery.finish() }
    }

    // 接收应用上下文里的清单
    private func acceptContext(_ data: Data, permitsReceipt: Bool = true) {
        let receiptForegroundId = permitsReceipt ? foregroundGate.capture() : nil
        // 在队列上异步处理
        queue.async { [self] in
            // 结束时处理已就绪的投递
            defer { finishReadyDeliveries() }
            // 尝试处理
            do {
                // 作为上下文来源接收清单
                let changed = try cache.receiveManifest(data, origin: .context)
                // 仅新的真实投递可以成为主模型的接纳事件，启动重读applicationContext不构造同步。
                didChange(nil, changed && permitsReceipt ? data : nil, receiptForegroundId)
            // 系统可能投递过期的排队代，属预期情况
            } catch CacheError.stale, CacheError.unknownStream {
                // 系统投递已过时的排队代次时属于预期情况。
            } catch { didChange(error.localizedDescription, nil, nil) }
        }
    }
}
