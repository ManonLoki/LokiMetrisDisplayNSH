import Foundation

@_silgen_name("loki_core_call")
private func lokiCoreCall(_ request: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?
@_silgen_name("loki_core_free")
private func lokiCoreFree(_ value: UnsafeMutablePointer<CChar>?)

/// Rust 展示核心（`crates/widget-core`）的唯一入口：校验、展示文字、共享摘要存储与小组件 HTTP
/// 协议都在 Rust；Swift 只做系统 API、网络传输与绘制。
enum LokiCore {
    struct Failure: Error, CustomStringConvertible { let description: String }

    /// 一个 JSON 请求换一个结果；Rust 侧错误按原文抛出，不含凭据或路径。
    static func call(_ request: [String: Any]) throws -> Any {
        let data = try JSONSerialization.data(withJSONObject: request)
        guard let raw = String(decoding: data, as: UTF8.self).withCString({ lokiCoreCall($0) }) else {
            throw Failure(description: "展示核心不可用")
        }
        defer { lokiCoreFree(raw) }
        let reply = try JSONSerialization.jsonObject(with: Data(bytes: raw, count: strlen(raw)), options: [.fragmentsAllowed])
        guard let reply = reply as? [String: Any] else { throw Failure(description: "展示核心返回无效") }
        if let error = reply["error"] as? String { throw Failure(description: error) }
        return reply["ok"] ?? NSNull()
    }

    /// 结果为 null 时返回 nil，其余按 Decodable 解析。
    static func decode<T: Decodable>(_ type: T.Type, _ request: [String: Any]) throws -> T? {
        let value = try call(request)
        if value is NSNull { return nil }
        let data = try JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed])
        return try SharedJSON.decoder.decode(T.self, from: data)
    }

    /// Encodable 值转成可放进请求的 JSON 对象。
    static func object<T: Encodable>(_ value: T) throws -> Any {
        try JSONSerialization.jsonObject(with: SharedJSON.encoder.encode(value), options: [.fragmentsAllowed])
    }
}

/// 不在 Swift 解释的 JSON 字段（Token、额度）；校验与展示交给 Rust。
enum JSONValue: Codable, Equatable {
    case null, bool(Bool), string(String), number(Double), integer(Int64), array([JSONValue]), object([String: JSONValue])

    init(from decoder: Decoder) throws {
        let value = try decoder.singleValueContainer()
        if value.decodeNil() { self = .null }
        else if let bool = try? value.decode(Bool.self) { self = .bool(bool) }
        else if let integer = try? value.decode(Int64.self) { self = .integer(integer) }
        else if let number = try? value.decode(Double.self) { self = .number(number) }
        else if let string = try? value.decode(String.self) { self = .string(string) }
        else if let array = try? value.decode([JSONValue].self) { self = .array(array) }
        else { self = .object(try value.decode([String: JSONValue].self)) }
    }

    func encode(to encoder: Encoder) throws {
        var value = encoder.singleValueContainer()
        switch self {
        case .null: try value.encodeNil()
        case .bool(let bool): try value.encode(bool)
        case .string(let string): try value.encode(string)
        case .number(let number): try value.encode(number)
        case .integer(let integer): try value.encode(integer)
        case .array(let array): try value.encode(array)
        case .object(let object): try value.encode(object)
        }
    }
}

/// Rust `view::project` 的结果：所有文字、排序、筛选与阈值已决定，界面只按字段绘制。
struct DisplayView: Codable, Equatable {
    struct Amount: Codable, Equatable {
        let text: String
        let compact: String
        let exact: String?
        let accessibilityLabel: String
    }
    struct Tokens: Codable, Equatable {
        let date: String
        let total: Amount
        let dateAccessibility: String
    }
    struct Window: Codable, Equatable {
        let kind: String
        let label: String
        let percent: String
        /// 0 红、1 黄、2 绿。
        let level: Int
        let remainingTenths: Int
        let accessibilityLabel: String
    }
    struct Provider: Codable, Equatable {
        let provider: String
        let name: String
        let plan: String?
        let windows: [Window]
        let description: String
    }
    struct Quota: Codable, Equatable {
        let observedAtUnix: Int64
        let providers: [Provider]
        var observedDate: Date { Date(timeIntervalSince1970: TimeInterval(observedAtUnix)) }
    }
    struct AgentRow: Codable, Equatable, Identifiable {
        let id: String
        let name: String
        let plan: String?
        let text: String
        let compact: String
        let exact: String?
        let accessibilityLabel: String
    }
    let hookCountText: String
    let hooksText: String
    let hooksSummary: String
    let hookLead: String?
    let tokens: Tokens?
    let tokenFallback: String
    let tokenAccessibilityLabel: String
    let quota: Quota?
    let agents: [AgentRow]
    let agentsEmptyText: String

    struct Labels: Decodable {
        let widgetEmptyInline: String
        let widgetEmptyCircular: String
        let widgetEmptyTitle: String
        let widgetEmptyHelp: String
        let appleSyncText: String
        let appleSyncDescription: String
        let appleCompactSyncText: String
        let appleCompactSyncDescription: String
        let quotaUpdate: String
        let quotaCompactUpdate: String
        let remainingAgents: String
        let providerDescriptions: [String: String]
        let appleAccessoryInline: String
        let appleAccessoryCircular: String
    }

    /// 无缓存时也读取共享空态投影，系统视图不另写一套缺失状态规则。
    static func emptyLabels(appName: String = "诡秘神谕") -> Labels? {
        try? LokiCore.decode(Labels.self, ["op": "view.describe", "view": NSNull(), "appName": appName])
    }

    /// 日期由 Foundation 按当前区域格式化，完整描述仍由同一 Rust 模板产生。
    func labels(syncTime: String? = nil, compactSyncTime: String? = nil, quotaTime: String? = nil,
                quotaCompactTime: String? = nil, visibleAgents: Int? = nil) throws -> Labels {
        guard let labels = try LokiCore.decode(Labels.self, ["op": "view.describe", "view": LokiCore.object(self),
            "syncTime": syncTime as Any? ?? NSNull(), "compactSyncTime": compactSyncTime as Any? ?? NSNull(),
            "quotaTime": quotaTime as Any? ?? NSNull(), "quotaCompactTime": quotaCompactTime as Any? ?? NSNull(),
            "visibleAgents": visibleAgents as Any? ?? NSNull()]) else { throw LokiCore.Failure(description: "展示描述无效") }
        return labels
    }
}

/// App 与组件共用的毫秒时间戳换算。
extension Date {
    init(epochMs: Int64) { self.init(timeIntervalSince1970: Double(epochMs) / 1000) }
    var epochMs: Int64 { Int64(timeIntervalSince1970 * 1000) }
}

/// 组件的中文同步时间格式；静态保存，避免视图每次渲染重建 Locale。
enum SyncTimeFormat {
    static let locale = Locale(identifier: "zh_CN")
    /// 读屏使用的完整年月日时分。
    static let full = Date.FormatStyle.dateTime.locale(locale).year().month().day().hour().minute()
    /// 空间有限时省略年份。
    static let short = Date.FormatStyle.dateTime.locale(locale).month().day().hour().minute()
}
