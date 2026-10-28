// 引入 Foundation
import Foundation

/// 仅包含公开的显示数据。手机配对记录和路径绝不会进入这个模型。
// 清单：只含公开的展示数据，手机的配对记录和路径绝不进入这个模型
struct CompanionManifest: Codable, Equatable {
    // 版本号
    let version: Int
    // 数据流 ID
    let streamId: String
    // 修订号（十进制文本）
    let revision: String
    // 来源桌面
    let sourceId: String?
    // 手机接收快照的时间
    let receivedAtMs: UInt64?
    // 快照
    let snapshot: Snapshot?
    // 资源描述列表
    let assets: [AssetDescriptor]

    // 数值化的修订号，必须为正
    var sequence: UInt64? { canonicalUnsigned(revision).flatMap { $0 > 0 ? $0 : nil } }
    // 是否为“清空”状态：没有来源、快照、时间和资源
    var isClear: Bool { sourceId == nil && snapshot == nil && receivedAtMs == nil && assets.isEmpty }

    // 校验清单
    func validate() throws {
        // 版本、流 ID、修订号、资源数量与摘要不重复
        guard version == 1, canonicalUUID(streamId), sequence != nil,
              // 资源不超过 12 个且哈希不重复
              assets.count <= 12, Set(assets.map(\.hash)).count == assets.count else {
            // 抛出清单无效
            throw CacheError.invalidManifest
        }
        // 清空状态到此为止
        if isClear { return }
        // 否则来源、时间和快照都必须齐全且合法
        guard let sourceId, canonicalUUID(sourceId),
              // 接收时间须为正且不超过安全整数
              let receivedAtMs, receivedAtMs > 0, receivedAtMs <= 9_007_199_254_740_991,
              // 须有快照
              let snapshot else { throw CacheError.invalidManifest }
        // 校验快照
        try snapshot.validate()
        // 逐个校验资源
        for asset in assets { try asset.validate() }
        // 快照引用的图片必须和资源清单完全一致
        let references = Set(snapshot.hooks.compactMap(\.image))
        // 资源哈希集合须与引用一致
        guard Set(assets.map(\.hash)) == references else {
            // 抛出清单无效
            throw CacheError.invalidManifest
        }
    }
}

// 图片资源描述：摘要、类型与长度
struct AssetDescriptor: Codable, Equatable {
    // 内容哈希
    let hash: String
    // 媒体类型
    let mime: String
    // 长度
    let length: UInt64

    // 摘要合法，长度 1 字节到 32 MiB，类型在白名单内
    func validate() throws {
        // 哈希合法且长度在 1 字节到 32 MiB
        guard validHash(hash), (1...32 * 1024 * 1024).contains(length),
              // 媒体类型须是受支持的图片类型
              ["image/png", "image/gif", "image/jpeg", "image/bmp", "image/webp"].contains(mime) else {
            // 抛出资源无效
            throw CacheError.invalidAsset
        }
    }
}

// 快照
struct Snapshot: Codable, Equatable {
    // 版本号
    let version: Int
    // 用量
    let tokens: TokenUsage?
    // 用量错误
    let tokenError: String?
    // Hook 是否可用
    let hooksAvailable: Bool
    // Hook 列表
    let hooks: [HookItem]
    // 额度
    let providerQuota: ProviderQuota?
    // 启用的代理名单
    let enabledAgents: [String]?

    // 版本、Hook 数量与槽位唯一、Hooks 不可用时必须为空、错误与统计互斥、启用列表长度
    func validate() throws {
        // 版本须为 1 且 Hook 不超过 12 个
        guard version == 1, hooks.count <= 12,
              // 槽位不重复
              Set(hooks.map(\.slot)).count == hooks.count,
              // 不可用时不得有 Hook
              hooksAvailable || hooks.isEmpty,
              // 有用量错误时不得有用量，且错误不超过 512 字节
              tokenError == nil || (tokens == nil && tokenError!.utf8.count <= 512),
              // 启用名单不超过 14 个
              (enabledAgents?.count ?? 0) <= 14 else { throw CacheError.invalidManifest }
        // 校验用量
        try tokens?.validate()
        // 校验额度
        try providerQuota?.validate()
        // 逐条校验 Hook
        for hook in hooks { try hook.validate() }
    }
}

// Token 统计
struct TokenUsage: Codable, Equatable {
    // 统计日
    struct Day: Codable, Equatable { let date: String; let zone: String }
    // 单个代理的用量
    struct Agent: Codable, Equatable, Identifiable {
        // 代理名
        let agent: String
        // 金额
        let amount: TokenAmount
        // 以代理名作标识
        var id: String { agent }
    }
    // 日期
    let day: Day
    // 总量
    let all: TokenAmount
    // 各代理
    let agents: [Agent]

    // 日期格式、时区长度、代理数量与名称唯一
    func validate() throws {
        // 日期、时区长度与代理数量须合法
        guard validDate(day.date), day.zone.utf8.count <= 256, agents.count <= 14,
              // 代理名不重复
              Set(agents.map(\.agent)).count == agents.count else { throw CacheError.invalidManifest }
        // 校验总量
        try all.validate()
        // 逐个校验代理
        for entry in agents {
            // 代理名非空且不超过 64 字节
            guard !entry.agent.isEmpty, entry.agent.utf8.count <= 64 else { throw CacheError.invalidManifest }
            // 校验金额
            try entry.amount.validate()
        }
    }
}

// 一项用量
struct TokenAmount: Codable, Equatable {
    // 总量文本
    let total: String?
    // 可用性
    let availability: String
    // 完整性
    let completeness: String
    // 置信度
    let confidence: String

    // 状态字段取值合法；可用时必须有规范数字，否则不能有数值
    func validate() throws {
        // 可用性须为三种取值之一
        guard ["available", "noRecords", "unavailable"].contains(availability),
              // 完整性须是三种取值之一
              ["complete", "partial", "unknown"].contains(completeness),
              // 置信度须是三种取值之一
              ["exact", "derived", "suspected"].contains(confidence),
              // 可用时总量须是规范无符号数
              availability == "available" ? (total.flatMap(canonicalUnsigned) != nil) : total == nil else {
            // 抛出清单无效
            throw CacheError.invalidManifest
        }
    }
}

// 一条 Hook
struct HookItem: Codable, Equatable, Identifiable {
    // 代理名
    let agent: String
    // 槽位
    let slot: Int
    // 行为
    let behavior: String
    // 内容
    let content: String
    // 内容是否被截断
    let contentTruncated: Bool
    // 图片哈希
    let image: String?
    // 以槽位作标识
    var id: Int { slot }

    // 槽位 1 到 12、代理名、行为、正文长度与图片摘要
    func validate() throws {
        // 槽位 1 到 12，代理名非空且不超过 64 字节
        guard (1...12).contains(slot), !agent.isEmpty, agent.utf8.count <= 64,
              // 行为须是四种取值之一，内容长度受限
              ["idle", "running", "asking", "error"].contains(behavior), content.utf8.count <= 512,
              // 图片哈希须合法
              image == nil || validHash(image!) else { throw CacheError.invalidManifest }
    }
}

// 供应商额度
struct ProviderQuota: Codable, Equatable {
    // 观测时间戳
    let observedAtUnix: UInt64
    // 供应商列表
    let providers: [QuotaProvider]

    // 供应商必须按固定顺序出现且不重复
    func validate() throws {
        // 供应商固定顺序
        let order = ["codex", "claude", "cursor", "grok"]
        // 各供应商在固定顺序里的位置
        let positions = providers.compactMap { order.firstIndex(of: $0.provider) }
        // 时间戳不超过安全整数，供应商不超过 4 个
        guard observedAtUnix <= 9_007_199_254_740_991, providers.count <= 4,
              // 位置数量一致且顺序递增
              positions.count == providers.count, positions == positions.sorted(),
              // 位置不重复
              Set(positions).count == positions.count else { throw CacheError.invalidManifest }
        // 逐个校验供应商
        for provider in providers { try provider.validate() }
    }
}

// 单个供应商
struct QuotaProvider: Codable, Equatable, Identifiable {
    // 供应商名
    let provider: String
    // 套餐
    let plan: String?
    // 五小时窗口
    let fiveHour: QuotaWindow?
    // 每周窗口
    let weekly: QuotaWindow?
    // 每月窗口
    let monthly: QuotaWindow?
    // 以供应商名作标识
    var id: String { provider }
    // 存在的窗口，顺序为五小时、每周、每月
    var windows: [QuotaWindow] { [fiveHour, weekly, monthly].compactMap { $0 } }

    // 名称在白名单内、至少一个窗口、套餐只含短小的字母数字与 _ . - 空格
    func validate() throws {
        // 供应商须在固定名单内
        guard ["codex", "claude", "cursor", "grok"].contains(provider),
              // 至少有一个窗口
              !windows.isEmpty,
              // 套餐长度与字符须合法
              plan == nil || (1...24).contains(plan!.utf8.count) && plan!.utf8.allSatisfy({
                  // 套餐字符须为字母数字或允许的符号
                  (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || [32, 95, 46, 45].contains($0)
              // 否则抛出清单无效
              }) else { throw CacheError.invalidManifest }
        // 窗口种类与字段一致，已用千分比在 0 到 1000
        for (kind, window) in [("fiveHour", fiveHour), ("weekly", weekly), ("monthly", monthly)] {
            // 窗口种类须一致且用量在 0 到 1000
            if let window, window.kind != kind || !(0...1000).contains(window.usedTenths) {
                // 抛出清单无效
                throw CacheError.invalidManifest
            }
        }
    }
}

// 额度窗口
struct QuotaWindow: Codable, Equatable, Identifiable {
    // 窗口种类
    let kind: String
    // 已用十分比
    let usedTenths: Int
    // 以种类作标识
    var id: String { kind }
}

// 缓存错误，附面向用户的中文说明
enum CacheError: Error, LocalizedError {
    // 缓存错误种类
    case invalidManifest, stale, conflictingRevision, unknownStream, invalidAsset, storageLimit
    // 错误描述
    var errorDescription: String? {
        // 按错误种类返回文案
        switch self {
        // 清单格式无效
        case .invalidManifest: return "同步数据格式无效"
        // 较旧的同步数据
        case .stale: return "已忽略较旧的同步数据"
        // 版本冲突
        case .conflictingRevision: return "同步版本冲突"
        // 等待 iPhone 的最新同步状态
        case .unknownStream: return "等待 iPhone 的最新同步状态"
        // 图片校验失败
        case .invalidAsset: return "图片校验未通过"
        // 缓存空间不足
        case .storageLimit: return "缓存空间不足"
        }
    }
}

// 规范的无符号整数：纯数字且无前导零
func canonicalUnsigned(_ text: String) -> UInt64? {
    // 非空且全为数字
    guard !text.isEmpty, text.utf8.allSatisfy({ (48...57).contains($0) }),
          // “0”或不以 0 开头才是规范数字
          text == "0" || !text.hasPrefix("0") else { return nil }
    // 转为无符号整数
    return UInt64(text)
}

// 64 位小写十六进制
func validHash(_ text: String) -> Bool {
    // 长度为 64 的小写十六进制
    text.utf8.count == 64 && text.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
}

// 规范小写 UUID
func canonicalUUID(_ text: String) -> Bool {
    // 规范写法的小写 UUID
    UUID(uuidString: text)?.uuidString.lowercased() == text
}

// 日期形如 YYYY-MM-DD
func validDate(_ text: String) -> Bool {
    // 长度为 10 且各位符合日期格式
    text.utf8.count == 10 && text.utf8.enumerated().allSatisfy { index, byte in
        // 第 5 位与第 8 位须为连字符，其余为数字
        index == 4 || index == 7 ? byte == 45 : (48...57).contains(byte)
    }
}
