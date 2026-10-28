// 引入摘要、Foundation 与 Darwin
import CryptoKit
// 基础库
import Foundation
// 系统库
import Darwin

/// 手表 UI 唯一的持久化输入。所有状态变更都由这把锁串行化。
// 手表界面唯一的持久输入；所有状态变化都由同一把锁串行化
final class SnapshotCache: @unchecked Sendable {
    // 清单来源：应用上下文或文件
    enum Origin { case context, file }
    // 持久状态：当前清单与已退役的数据流
    private struct State: Codable {
        // 当前清单
        var manifest: CompanionManifest?
        // 已废弃的流
        var retiredStreams: [String]
    }
    // 待安装图片的记录（先于清单到达的图片）
    private struct Pending: Codable {
        // 所属流标识
        let streamId: String
        // 修订号
        let revision: String
        // 哈希
        let hash: String
    }
    // 缓存根目录
    private let root: URL
    // 锁
    private let lock = NSLock()
    // 状态
    private var state = State(manifest: nil, retiredStreams: [])
    // 文件管理器
    private let files = FileManager.default
    // 磁盘预算 128 MiB
    private static let budget: UInt64 = 128 * 1024 * 1024

    // 创建目录，读取并校验已保存的状态
    init(root: URL) throws {
        // 保存根目录
        self.root = root
        // 创建目录
        try files.createDirectory(at: root, withIntermediateDirectories: true)
        // 三个子目录：已确认图片、待安装图片、刚收到的临时文件
        for name in ["assets", "pending", "incoming"] {
            // 创建子目录
            try files.createDirectory(at: root.appendingPathComponent(name), withIntermediateDirectories: true)
        }
        // 状态文件不超过 64 KiB，退役流不超过 128 个且都是 UUID
        if let data = try? Data(contentsOf: root.appendingPathComponent("state.json")), data.count <= 64 * 1024,
           // 解码已保存的状态
           let loaded = try? JSONDecoder().decode(State.self, from: data), loaded.retiredStreams.count <= 128,
           // 已废弃的流标识都须是 UUID
           loaded.retiredStreams.allSatisfy({ UUID(uuidString: $0) != nil }) {
            // 尝试校验已加载的状态
            do {
                // 清单必须有效
                try loaded.manifest?.validate()
                // 采用该状态
                state = loaded
            // 损坏的本地文件绝不授权旧的展示数据；新的上下文可以修复它
            } catch {
                // 损坏的本地文件不会授权使用旧的显示数据;新的上下文可以修复它。
            }
        }
        // 从 WCSession 回调移动过来的文件,在下面归并之前不算可信状态。
        // 从 WCSession 回调搬来的文件在这里被清掉，不当作可信状态
        try removeChildren(in: root.appendingPathComponent("incoming"))
        // 清理多余文件
        try prune()
    }

    // 当前清单
    var manifest: CompanionManifest? { locked { state.manifest } }

    // 调用方可忽略返回值
    @discardableResult
    // 接收清单：校验后按流与修订号决定接受、忽略还是报冲突
    func receiveManifest(_ data: Data, origin: Origin) throws -> Bool {
        // 在锁内执行
        try locked {
            // 清单不得超过 48 KiB
            guard data.count <= 48 * 1024 else { throw CacheError.invalidManifest }
            // 解码清单
            let next = try JSONDecoder().decode(CompanionManifest.self, from: data)
            // 校验新清单
            try next.validate()
            // 已有清单
            if let previous = state.manifest {
                // 同一数据流：修订号只能前进
                if next.streamId == previous.streamId {
                    // 序号必须不小于已有序号
                    guard let new = next.sequence, let old = previous.sequence, new >= old else { throw CacheError.stale }
                    // 相同修订号：内容必须完全一致，成功则顺便安装待处理图片
                    if new == old {
                        // 同一序号的内容必须一致
                        guard next == previous else { throw CacheError.conflictingRevision }
                        // 安装待处理资源
                        try installPending()
                        // 不接受
                        return false
                    }
                // 不同数据流：只接受来自应用上下文且未退役的，退役数量有上限
                } else {
                    // 上下文来源且流未被废弃才接受
                    guard origin == .context, !state.retiredStreams.contains(next.streamId) else {
                        // 否则报未知流
                        throw CacheError.unknownStream
                    }
                    // 已废弃的流不得超过 128 条
                    guard state.retiredStreams.count < 128 else { throw CacheError.storageLimit }
                }
            // 还没有任何清单时，文件来源的不被接受
            } else if origin != .context {
                // 报未知流
                throw CacheError.unknownStream
            }
            // 复制当前状态
            var nextState = state
            // 换流时把旧流登记为退役
            if let previous = state.manifest, previous.streamId != next.streamId {
                // 旧流记入已废弃列表
                nextState.retiredStreams.append(previous.streamId)
            }
            // 采用新清单
            nextState.manifest = next
            // 先把清除/新一代的状态持久化,再去动旧文件。
            // 先持久化新的一代（含清空），再动旧文件
            try save(nextState)
            // 采用新状态
            state = nextState
            // 清理多余文件
            try prune()
            // 安装待处理资源
            try installPending()
            // 返回已接受
            return true
        }
    }

    /// 在 WCSession 的委托回调返回之前,先移走它的临时 URL。
    // 在 WCSession 回调返回之前，把系统的临时文件搬到缓存里
    func stageIncoming(_ temporary: URL) throws -> URL {
        // 在锁内执行
        try locked {
            // 必须是普通文件、非符号链接、大小在范围内，且不超过磁盘预算
            let facts = try temporary.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
            // 须是普通文件且不是链接
            guard facts.isRegularFile == true, facts.isSymbolicLink != true,
                  // 大小在 1 字节到 32 MiB
                  let size = facts.fileSize, (1...32 * 1024 * 1024).contains(size),
                  // 加上已用空间不得超过预算
                  try diskUsage() + UInt64(size) <= Self.budget else { throw CacheError.storageLimit }
            // 暂存目录
            let directory = root.appendingPathComponent("incoming")
            // 暂存文件数量有上限
            guard try files.contentsOfDirectory(atPath: directory.path).count < 12 else { throw CacheError.storageLimit }
            // 以随机名作为暂存文件
            let destination = directory.appendingPathComponent(UUID().uuidString)
            // 移入目的位置
            try files.moveItem(at: temporary, to: destination)
            // 返回目的位置
            return destination
        }
    }

    // 调用方可忽略返回值
    @discardableResult
    // 接收图片：校验摘要；属于当前流且已不新于当前修订的立即安装，否则放入待处理
    func receiveAsset(_ staged: URL, streamId: String, revision: String, hash: String) throws -> Bool {
        // 在锁内执行
        try locked {
            // 结束时删除暂存文件
            defer { try? files.removeItem(at: staged) }
            // 参数必须合法，且流未退役
            guard UUID(uuidString: streamId) != nil, let sequence = canonicalUnsigned(revision), sequence > 0,
                  // 哈希合法且流未被废弃
                  validHash(hash), !state.retiredStreams.contains(streamId) else { throw CacheError.invalidAsset }
            // 检查文件内容与摘要
            let facts = try inspect(staged)
            // 文件哈希须一致
            guard facts.hash == hash else { throw CacheError.invalidAsset }
            // 属于当前流：清单里要有对应描述，长度和类型一致才安装
            if let current = state.manifest, current.streamId == streamId, sequence <= current.sequence! {
                // 清单里有该哈希的描述
                guard let descriptor = current.assets.first(where: { $0.hash == hash }),
                      // 长度与类型须与描述一致
                      descriptor.length == facts.length, descriptor.mime == facts.mime else { throw CacheError.invalidAsset }
                // 校验后移入资源目录
                try moveVerified(staged, to: assetURL(hash))
                // 返回已接受
                return true
            }
            // 否则作为待处理保存，数量有上限
            let pending = root.appendingPathComponent("pending")
            // 列出待处理目录
            let entries = try files.contentsOfDirectory(at: pending, includingPropertiesForKeys: nil)
                // 只看 json 附属文件
                .filter { $0.pathExtension == "json" }
            // 待处理文件不得超过 12 个
            guard entries.count < 12 else { throw CacheError.storageLimit }
            // 文件键：流、修订号与哈希
            let key = "\(streamId)-\(revision)-\(hash)"
            // 目的路径
            let destination = pending.appendingPathComponent(key)
            // 移入待处理目录
            try files.moveItem(at: staged, to: destination)
            // 同时写一份元数据文件
            try JSONEncoder().encode(Pending(streamId: streamId, revision: revision, hash: hash))
                // 写入附属记录
                .write(to: destination.appendingPathExtension("json"), options: .atomic)
            // 返回待处理
            return false
        }
    }

    /// 只返回当前清单所指定的、已验证的本地图片。
    // 只返回当前清单点名且已校验的本地图片
    func imageURL(hash: String) -> URL? {
        // 在锁内执行
        locked {
            // 重新检查摘要、长度和类型
            guard let descriptor = state.manifest?.assets.first(where: { $0.hash == hash }),
                  // 校验已有文件
                  let facts = try? inspect(assetURL(hash)), facts.hash == hash,
                  // 长度与类型须一致
                  facts.length == descriptor.length, facts.mime == descriptor.mime else { return nil }
            // 返回资源路径
            return assetURL(hash)
        }
    }

    // 安装待处理图片：满足条件的移入图片目录，已过期的删除，其余保留等待
    private func installPending() throws {
        // 没有清单则返回
        guard let current = state.manifest else { return }
        // 待处理目录
        let directory = root.appendingPathComponent("pending")
        // 遍历附属记录
        for sidecar in try files.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)
            // 只处理 json 文件
            where sidecar.pathExtension == "json" {
            // 元数据损坏则删除
            guard let data = try? Data(contentsOf: sidecar), data.count < 1024,
                  // 解码待处理记录
                  let item = try? JSONDecoder().decode(Pending.self, from: data) else {
                // 删除附属文件
                try? files.removeItem(at: sidecar)
                // 跳过
                continue
            }
            // 对应的图片文件
            let image = sidecar.deletingPathExtension()
            // 同一流且修订号已到：校验后安装，然后清理
            if item.streamId == current.streamId && (canonicalUnsigned(item.revision) ?? UInt64.max) <= current.sequence! {
                // 清单里有该资源的描述
                if let descriptor = current.assets.first(where: { $0.hash == item.hash }),
                   // 校验图片
                   let facts = try? inspect(image), facts.hash == descriptor.hash,
                   // 长度与类型须一致
                   facts.length == descriptor.length, facts.mime == descriptor.mime {
                    // 校验后移入资源目录
                    try moveVerified(image, to: assetURL(item.hash))
                }
                // 否则删除图片
                try? files.removeItem(at: image)
                // 删除附属记录
                try? files.removeItem(at: sidecar)
            // 属于已退役的流，或比当前修订更旧：直接删除
            } else if state.retiredStreams.contains(item.streamId)
                // 同一流且修订号不过期
                || (item.streamId == current.streamId && (canonicalUnsigned(item.revision) ?? 0) < current.sequence!) {
                // 删除图片
                try? files.removeItem(at: image)
                // 删除附属记录
                try? files.removeItem(at: sidecar)
            }
        }
    }

    // 清理：删除清单不再引用的图片；清空状态还要清空待处理目录
    private func prune() throws {
        // 需要保留的资源哈希
        let keep = Set(state.manifest?.assets.map(\.hash) ?? [])
        // 遍历资源目录
        for file in try files.contentsOfDirectory(at: root.appendingPathComponent("assets"), includingPropertiesForKeys: nil)
            // 不在保留名单里的
            where !keep.contains(file.lastPathComponent) {
            // 删除不再需要的文件
            try? files.removeItem(at: file)
        }
        // 清单是清除指令时清空暂存目录
        if state.manifest?.isClear == true { try removeChildren(in: root.appendingPathComponent("pending")) }
    }

    // 原子写入状态文件
    private func save(_ next: State) throws {
        // 把新状态写入 state.json
        try JSONEncoder().encode(next).write(to: root.appendingPathComponent("state.json"), options: .atomic)
    }

    // 图片的缓存路径
    private func assetURL(_ hash: String) -> URL { root.appendingPathComponent("assets/\(hash)") }

    // 两个路径都在缓存内的同一个卷上，rename 原子地替换旧文件
    private func moveVerified(_ source: URL, to destination: URL) throws {
        // 两个 URL 都在同一个卷上的本缓存内,rename 会原子地替换任何旧副本。
        guard Darwin.rename(source.path, destination.path) == 0 else { throw CocoaError(.fileWriteUnknown) }
    }

    // 检查文件：普通文件、大小范围，读取前缀判断类型，并流式计算 SHA-256
    private func inspect(_ url: URL) throws -> (hash: String, mime: String, length: UInt64) {
        // 读取资源值：是否普通文件、是否符号链接
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        // 须是普通文件且不是链接
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              // 大小在 1 字节到 32 MiB
              let size = values.fileSize, (1...32 * 1024 * 1024).contains(size) else { throw CacheError.invalidAsset }
        // 打开文件读取
        let handle = try FileHandle(forReadingFrom: url)
        // 结束时关闭文件
        defer { try? handle.close() }
        // 按魔数判断类型：PNG、GIF、JPEG、BMP、WebP
        let prefix = try handle.read(upToCount: 16) ?? Data()
        // 媒体类型
        let mime: String
        // PNG 文件头
        if prefix.starts(with: [137,80,78,71,13,10,26,10]) { mime = "image/png" }
        // GIF 文件头
        else if prefix.starts(with: Data("GIF87a".utf8)) || prefix.starts(with: Data("GIF89a".utf8)) { mime = "image/gif" }
        // JPEG 文件头
        else if prefix.starts(with: [255,216,255]) { mime = "image/jpeg" }
        // BMP 文件头
        else if prefix.starts(with: [66,77]) { mime = "image/bmp" }
        // RIFF 容器（WebP）
        else if prefix.starts(with: Data("RIFF".utf8)), prefix.count >= 12,
                // WebP 标识
                prefix[8..<12] == Data("WEBP".utf8) { mime = "image/webp" }
        // 无法识别的类型
        else { throw CacheError.invalidAsset }
        // 初始化摘要
        var digest = SHA256()
        // 更新摘要
        digest.update(data: prefix)
        // 累计字节数
        var count = UInt64(prefix.count)
        // 每次 64 KiB 地读完并累计
        while let bytes = try handle.read(upToCount: 64 * 1024), !bytes.isEmpty {
            // 累计字节数
            count += UInt64(bytes.count)
            // 超过 32 MiB 则抛出资源无效
            guard count <= 32 * 1024 * 1024 else { throw CacheError.invalidAsset }
            // 更新摘要
            digest.update(data: bytes)
        }
        // 读到的总数必须与文件大小一致
        guard count == size else { throw CacheError.invalidAsset }
        // 返回十六进制摘要、类型与大小
        return (digest.finalize().map { String(format: "%02x", $0) }.joined(), mime, count)
    }

    // 缓存目录当前占用的字节数
    private func diskUsage() throws -> UInt64 {
        // 需要读取的资源键
        let keys: [URLResourceKey] = [.fileSizeKey, .isRegularFileKey]
        // 枚举目录
        guard let enumerator = files.enumerator(at: root, includingPropertiesForKeys: keys) else { return 0 }
        // 累加目录内所有普通文件的大小
        return try enumerator.compactMap { $0 as? URL }.reduce(0) { total, url in
            // 读取资源值
            let values = try url.resourceValues(forKeys: Set(keys))
            // 累加普通文件大小
            return total + UInt64(values.isRegularFile == true ? max(0, values.fileSize ?? 0) : 0)
        }
    }

    // 删除目录下的所有子项
    private func removeChildren(in directory: URL) throws {
        // 遍历目录内容
        for file in try files.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil) { try files.removeItem(at: file) }
    }

    // 在锁内执行
    private func locked<T>(_ operation: () throws -> T) rethrows -> T {
        // 加锁
        lock.lock()
        // 结束时解锁
        defer { lock.unlock() }
        // 执行操作
        return try operation()
    }
}
