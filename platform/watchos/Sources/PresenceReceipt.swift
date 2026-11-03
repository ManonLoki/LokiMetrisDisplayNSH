import Foundation

/// 手机发起的当前快照探针；只承载身份与挑战，不改变已有后台缓存协议。
struct WatchPresenceProbe: Codable, Sendable {
    let version: Int
    let type: String
    let challenge: String
    let streamId: String
    let revision: String
    let sourceId: String

    /// 探针字段固定且有界；不接受缓存内容、未知字段或非规范身份伪造实时回执。
    static func decode(_ data: Data) throws -> Self {
        guard data.count <= 4096,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys) == Set(["version", "type", "challenge", "streamId", "revision", "sourceId"]) else {
            throw CacheError.invalidManifest
        }
        let value = try JSONDecoder().decode(Self.self, from: data)
        guard value.version == 1, value.type == "presenceProbe", canonicalUUID(value.challenge),
              canonicalUUID(value.streamId), canonicalUUID(value.sourceId),
              canonicalUnsigned(value.revision).isPositive else { throw CacheError.invalidManifest }
        return value
    }
}

/// 不含凭据的Watch安装身份；UUID与当前前台代次各自独立。
struct WatchPresenceInfo: Codable, Equatable, Sendable {
    let deviceId: String
    let name: String
    let appVersion: String
}

/// 最近一次当前前台接纳的完整新快照；年龄只由单调时钟计算。
struct WatchPresenceSync: Codable, Equatable, Sendable {
    let streamId: String
    let revision: String
    let sourceId: String
    let ageMs: UInt64
}

/// 探针回执不是桌面wire消息；手机验证挑战与代次后生成已有companionStatus。
struct WatchPresenceReply: Encodable, Sendable {
    let version = 1
    let type = "presenceReply"
    let challenge: String
    let info: WatchPresenceInfo
    let active: Bool
    let foregroundId: String?
    let sync: WatchPresenceSync?

    private enum CodingKeys: String, CodingKey {
        case version, type, challenge, info, active, foregroundId, sync
    }

    /// 显式编码空资格为null，避免合成encodeIfPresent省略合同字段。
    func encode(to encoder: Encoder) throws {
        var value = encoder.container(keyedBy: CodingKeys.self)
        try value.encode(version, forKey: .version)
        try value.encode(type, forKey: .type)
        try value.encode(challenge, forKey: .challenge)
        try value.encode(info, forKey: .info)
        try value.encode(active, forKey: .active)
        try value.encode(foregroundId, forKey: .foregroundId)
        try value.encode(sync, forKey: .sync)
    }
}

/// 尽力发送的撤销仅指向离开的那个前台代次；迟到撤销不能影响后继代次。
struct WatchPresenceRevocation: Encodable, Sendable {
    let version = 1
    let type = "presenceRevoked"
    let deviceId: String
    let foregroundId: String
}

/// 只由Watch主线程推进的纯状态；后台接纳、冷缓存重读和重复revision都不创建同步时刻。
struct WatchForegroundPresence {
    private struct Accepted {
        let streamId: String
        let revision: String
        let sourceId: String
        let at: ContinuousClock.Instant
    }
    private(set) var foregroundId: String?
    private var accepted: Accepted?

    /// 仅真正进入active创建新代次；重复active保持同一代，离开立即清除同步资格。
    mutating func setActive(_ active: Bool) -> String? {
        if active {
            if foregroundId == nil {
                foregroundId = UUID().uuidString.lowercased()
                accepted = nil
            }
            return nil
        }
        let revoked = foregroundId
        foregroundId = nil
        accepted = nil
        return revoked
    }

    /// 调用者已在前台主模型采用一个实际changed且验证通过的manifest；清空撤销旧同步事实。
    mutating func adoptedChanged(_ manifest: CompanionManifest, receiptForegroundId: String?, at: ContinuousClock.Instant) {
        guard let receiptForegroundId, receiptForegroundId == foregroundId else { return }
        guard !manifest.isClear, let sourceId = manifest.sourceId, manifest.snapshot != nil else {
            accepted = nil
            return
        }
        if let old = accepted, old.streamId == manifest.streamId,
           old.revision == manifest.revision, old.sourceId == sourceId { return }
        accepted = Accepted(streamId: manifest.streamId, revision: manifest.revision, sourceId: sourceId, at: at)
    }

    /// 回执只描述当前active；仅探针所指的同份快照可以附带原始接纳年龄，重复探针不刷新年龄。
    func reply(_ probe: WatchPresenceProbe, info: WatchPresenceInfo, at: ContinuousClock.Instant) -> WatchPresenceReply {
        var sync: WatchPresenceSync?
        if foregroundId != nil, let value = accepted,
           value.streamId == probe.streamId, value.revision == probe.revision, value.sourceId == probe.sourceId,
           let age = ageMilliseconds(value.at.duration(to: at)) {
            sync = WatchPresenceSync(streamId: value.streamId, revision: value.revision, sourceId: value.sourceId, ageMs: age)
        }
        return WatchPresenceReply(challenge: probe.challenge, info: info, active: foregroundId != nil,
                                  foregroundId: foregroundId, sync: sync)
    }
}

/// 向上取整到毫秒，历史年龄最多七天；负时长与溢出不构造同步事实。
private func ageMilliseconds(_ age: Duration) -> UInt64? {
    guard age >= .zero, age <= .seconds(7 * 24 * 60 * 60) else { return nil }
    let components = age.components
    guard components.seconds >= 0, components.attoseconds >= 0 else { return nil }
    let whole = UInt64(components.seconds) * 1000
    let fraction = UInt64(components.attoseconds)
    return whole + fraction / 1_000_000_000_000_000 + (fraction % 1_000_000_000_000_000 == 0 ? 0 : 1)
}

private extension Optional where Wrapped == UInt64 {
    /// 缺失或零revision不表示已接纳的快照。
    var isPositive: Bool { map { $0 > 0 } ?? false }
}

/// 接收入口用短锁快照真实前台代次；后台到达或上一代排队的消息不能在后继active中取得新资格。
final class WatchForegroundReceiptGate: @unchecked Sendable {
    private let lock = NSLock()
    private var foregroundId: String?

    /// 主线程仅在scenePhase变化后发布当前代次，不持锁执行任何WC回调。
    func set(_ id: String?) {
        lock.lock()
        foregroundId = id
        lock.unlock()
    }

    /// 系统回调入口立即捕获，只复制UUID字符串，不访问主线程模型或缓存。
    func capture() -> String? {
        lock.lock()
        defer { lock.unlock() }
        return foregroundId
    }
}
