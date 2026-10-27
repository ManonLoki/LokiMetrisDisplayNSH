import Foundation

/// Public display data only. Phone pairing records and paths never enter this model.
struct CompanionManifest: Codable, Equatable {
    let version: Int
    let streamId: String
    let revision: String
    let sourceId: String?
    let receivedAtMs: UInt64?
    let snapshot: DisplaySnapshot?
    let assets: [AssetDescriptor]

    var sequence: UInt64? { canonicalUnsigned(revision).flatMap { $0 > 0 ? $0 : nil } }
    var isClear: Bool { sourceId == nil && snapshot == nil && receivedAtMs == nil && assets.isEmpty }

    func validate() throws {
        guard version == 1, canonicalUUID(streamId), sequence != nil,
              assets.count <= 12, Set(assets.map(\.hash)).count == assets.count else {
            throw CacheError.invalidManifest
        }
        if isClear { return }
        guard let sourceId, canonicalUUID(sourceId),
              let receivedAtMs, receivedAtMs > 0, receivedAtMs <= 9_007_199_254_740_991,
              let snapshot else { throw CacheError.invalidManifest }
        try snapshot.validate()
        for asset in assets { try asset.validate() }
        let references = Set(snapshot.hooks.compactMap(\.image))
        guard Set(assets.map(\.hash)) == references else {
            throw CacheError.invalidManifest
        }
    }
}

struct AssetDescriptor: Codable, Equatable {
    let hash: String
    let mime: String
    let length: UInt64

    func validate() throws {
        guard validHash(hash), (1...32 * 1024 * 1024).contains(length),
              ["image/png", "image/gif", "image/jpeg", "image/bmp", "image/webp"].contains(mime) else {
            throw CacheError.invalidAsset
        }
    }
}

struct DisplaySnapshot: Codable, Equatable {
    let version: Int
    let tokens: TokenUsage?
    let tokenError: String?
    let hooksAvailable: Bool
    let hooks: [HookItem]
    let providerQuota: ProviderQuota?
    let enabledAgents: [String]?

    func validate() throws {
        guard version == 1, hooks.count <= 12,
              Set(hooks.map(\.slot)).count == hooks.count,
              hooksAvailable || hooks.isEmpty,
              tokenError == nil || (tokens == nil && tokenError!.utf8.count <= 512),
              (enabledAgents?.count ?? 0) <= 14 else { throw CacheError.invalidManifest }
        try tokens?.validate()
        try providerQuota?.validate()
        for hook in hooks { try hook.validate() }
    }
}

struct TokenUsage: Codable, Equatable {
    struct Day: Codable, Equatable { let date: String; let zone: String }
    struct Agent: Codable, Equatable, Identifiable {
        let agent: String
        let amount: TokenAmount
        var id: String { agent }
    }
    let day: Day
    let all: TokenAmount
    let agents: [Agent]

    func validate() throws {
        guard validDate(day.date), day.zone.utf8.count <= 256, agents.count <= 14,
              Set(agents.map(\.agent)).count == agents.count else { throw CacheError.invalidManifest }
        try all.validate()
        for entry in agents {
            guard !entry.agent.isEmpty, entry.agent.utf8.count <= 64 else { throw CacheError.invalidManifest }
            try entry.amount.validate()
        }
    }
}

struct TokenAmount: Codable, Equatable {
    let total: String?
    let availability: String
    let completeness: String
    let confidence: String

    func validate() throws {
        guard ["available", "noRecords", "unavailable"].contains(availability),
              ["complete", "partial", "unknown"].contains(completeness),
              ["exact", "derived", "suspected"].contains(confidence),
              availability == "available" ? (total.flatMap(canonicalUnsigned) != nil) : total == nil else {
            throw CacheError.invalidManifest
        }
    }
}

struct HookItem: Codable, Equatable, Identifiable {
    let agent: String
    let slot: Int
    let behavior: String
    let content: String
    let contentTruncated: Bool
    let image: String?
    var id: Int { slot }

    func validate() throws {
        guard (1...12).contains(slot), !agent.isEmpty, agent.utf8.count <= 64,
              ["idle", "running", "asking", "error"].contains(behavior), content.utf8.count <= 512,
              image == nil || validHash(image!) else { throw CacheError.invalidManifest }
    }
}

struct ProviderQuota: Codable, Equatable {
    let observedAtUnix: UInt64
    let providers: [QuotaProvider]

    func validate() throws {
        let order = ["codex", "claude", "cursor", "grok"]
        let positions = providers.compactMap { order.firstIndex(of: $0.provider) }
        guard observedAtUnix <= 9_007_199_254_740_991, providers.count <= 4,
              positions.count == providers.count, positions == positions.sorted(),
              Set(positions).count == positions.count else { throw CacheError.invalidManifest }
        for provider in providers { try provider.validate() }
    }
}

struct QuotaProvider: Codable, Equatable, Identifiable {
    let provider: String
    let plan: String?
    let fiveHour: QuotaWindow?
    let weekly: QuotaWindow?
    let monthly: QuotaWindow?
    var id: String { provider }
    var windows: [QuotaWindow] { [fiveHour, weekly, monthly].compactMap { $0 } }

    func validate() throws {
        guard ["codex", "claude", "cursor", "grok"].contains(provider),
              !windows.isEmpty,
              plan == nil || (1...24).contains(plan!.utf8.count) && plan!.utf8.allSatisfy({
                  (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || [32, 95, 46, 45].contains($0)
              }) else { throw CacheError.invalidManifest }
        for (kind, window) in [("fiveHour", fiveHour), ("weekly", weekly), ("monthly", monthly)] {
            if let window, window.kind != kind || !(0...1000).contains(window.usedTenths) {
                throw CacheError.invalidManifest
            }
        }
    }
}

struct QuotaWindow: Codable, Equatable, Identifiable {
    let kind: String
    let usedTenths: Int
    var id: String { kind }
}

enum CacheError: Error, LocalizedError {
    case invalidManifest, stale, conflictingRevision, unknownStream, invalidAsset, storageLimit
    var errorDescription: String? {
        switch self {
        case .invalidManifest: return "同步数据格式无效"
        case .stale: return "已忽略较旧的同步数据"
        case .conflictingRevision: return "同步版本冲突"
        case .unknownStream: return "等待 iPhone 的最新同步状态"
        case .invalidAsset: return "图片校验未通过"
        case .storageLimit: return "缓存空间不足"
        }
    }
}

func canonicalUnsigned(_ text: String) -> UInt64? {
    guard !text.isEmpty, text.utf8.allSatisfy({ (48...57).contains($0) }),
          text == "0" || !text.hasPrefix("0") else { return nil }
    return UInt64(text)
}

func validHash(_ text: String) -> Bool {
    text.utf8.count == 64 && text.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
}

func canonicalUUID(_ text: String) -> Bool {
    UUID(uuidString: text)?.uuidString.lowercased() == text
}

func validDate(_ text: String) -> Bool {
    text.utf8.count == 10 && text.utf8.enumerated().allSatisfy { index, byte in
        index == 4 || index == 7 ? byte == 45 : (48...57).contains(byte)
    }
}
