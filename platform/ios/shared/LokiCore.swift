// 引入 Foundation
import Foundation

// 绑定 Rust 导出的 C 函数：调用与释放
@_silgen_name("loki_core_call")
// 声明 C 函数：调用展示核心
private func lokiCoreCall(_ request: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?
// 声明 C 函数
@_silgen_name("loki_core_free")
// 声明 C 函数：释放返回的字符串
private func lokiCoreFree(_ value: UnsafeMutablePointer<CChar>?)

/// Rust 展示核心（`crates/widget-core`）的唯一入口：校验、展示文字、共享摘要存储与小组件 HTTP
/// 协议都在 Rust；Swift 只做系统 API、网络传输与绘制。
// Rust 展示核心的唯一入口：校验、展示文字、共享摘要存储与小组件 HTTP 协议都在 Rust，Swift 只做系统 API、网络传输与绘制
enum LokiCore {
    // 失败：带 Rust 侧的原文说明
    struct Failure: Error, CustomStringConvertible { let description: String }

    /// 一个 JSON 请求换一个结果；Rust 侧错误按原文抛出，不含凭据或路径。
    // 一个 JSON 请求换一个结果；Rust 侧的错误按原文抛出，不含凭据或路径
    static func call(_ request: [String: Any]) throws -> Any {
        // 把请求序列化为 JSON
        let data = try JSONSerialization.data(withJSONObject: request)
        // 调用 Rust，失败说明核心不可用
        guard let raw = String(decoding: data, as: UTF8.self).withCString({ lokiCoreCall($0) }) else {
            // 展示核心不可用
            throw Failure(description: "展示核心不可用")
        }
        // 用完释放 Rust 分配的字符串
        defer { lokiCoreFree(raw) }
        // 解析核心返回的 JSON
        let reply = try JSONSerialization.jsonObject(with: Data(bytes: raw, count: strlen(raw)), options: [.fragmentsAllowed])
        // 回复须是字典
        guard let reply = reply as? [String: Any] else { throw Failure(description: "展示核心返回无效") }
        // 有 error 就抛出
        if let error = reply["error"] as? String { throw Failure(description: error) }
        // 返回 ok 字段
        return reply["ok"] ?? NSNull()
    }

    /// 结果为 null 时返回 nil，其余按 Decodable 解析。
    // 结果为 null 时返回 nil，其余按 Decodable 解析
    static func decode<T: Decodable>(_ type: T.Type, _ request: [String: Any]) throws -> T? {
        // 调用核心
        let value = try call(request)
        // 空值返回 nil
        if value is NSNull { return nil }
        // 序列化结果
        let data = try JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed])
        // 解码为目标类型
        return try SharedJSON.decoder.decode(T.self, from: data)
    }

    /// Encodable 值转成可放进请求的 JSON 对象。
    // 把 Encodable 值转成可放进请求的 JSON 对象
    static func object<T: Encodable>(_ value: T) throws -> Any {
        // 经编码器往返得到 JSON 对象
        try JSONSerialization.jsonObject(with: SharedJSON.encoder.encode(value), options: [.fragmentsAllowed])
    }
}

/// 不在 Swift 解释的 JSON 字段（Token、额度）；校验与展示交给 Rust。
// 不在 Swift 里解释的 JSON 字段（Token、额度）；校验与展示交给 Rust
enum JSONValue: Codable, Equatable {
    // 各种 JSON 值
    case null, bool(Bool), string(String), number(Double), integer(Int64), array([JSONValue]), object([String: JSONValue])

    // 依次尝试解码：空、布尔、整数、浮点、字符串、数组，最后是对象
    init(from decoder: Decoder) throws {
        // 取单值容器
        let value = try decoder.singleValueContainer()
        // 空值
        if value.decodeNil() { self = .null }
        // 布尔
        else if let bool = try? value.decode(Bool.self) { self = .bool(bool) }
        // 整数
        else if let integer = try? value.decode(Int64.self) { self = .integer(integer) }
        // 浮点数
        else if let number = try? value.decode(Double.self) { self = .number(number) }
        // 字符串
        else if let string = try? value.decode(String.self) { self = .string(string) }
        // 数组
        else if let array = try? value.decode([JSONValue].self) { self = .array(array) }
        // 其余情形按对象解码
        else { self = .object(try value.decode([String: JSONValue].self)) }
    }

    // 编码：按类型分别写出
    func encode(to encoder: Encoder) throws {
        // 取单值容器
        var value = encoder.singleValueContainer()
        // 按类型编码
        switch self {
        // 空值
        case .null: try value.encodeNil()
        // 布尔
        case .bool(let bool): try value.encode(bool)
        // 字符串
        case .string(let string): try value.encode(string)
        // 浮点数
        case .number(let number): try value.encode(number)
        // 整数
        case .integer(let integer): try value.encode(integer)
        // 数组
        case .array(let array): try value.encode(array)
        // 对象
        case .object(let object): try value.encode(object)
        }
    }
}

/// Rust `view::project` 的结果：所有文字、排序、筛选与阈值已决定，界面只按字段绘制。
// Rust view::project 的结果：文字、排序、筛选与阈值都已决定，界面只按字段绘制
struct DisplayView: Codable, Equatable {
    // 一项用量：文本、紧凑、精确与无障碍标签
    struct Amount: Codable, Equatable {
        // 文本
        let text: String
        // 紧凑写法
        let compact: String
        // 精确写法
        let exact: String?
        // 读屏文案
        let accessibilityLabel: String
    }
    // Token 统计：日期、总量与日期的无障碍描述
    struct Tokens: Codable, Equatable {
        // 日期
        let date: String
        // 总量
        let total: Amount
        // 日期读屏文案
        let dateAccessibility: String
    }
    // 额度窗口：种类、标签、百分比、等级与剩余千分比
    struct Window: Codable, Equatable {
        // 种类
        let kind: String
        // 标签
        let label: String
        // 百分比
        let percent: String
        /// 0 红、1 黄、2 绿。
        let level: Int
        // 剩余十分比
        let remainingTenths: Int
        // 读屏文案
        let accessibilityLabel: String
    }
    // 供应商：名称、套餐、窗口与整体描述
    struct Provider: Codable, Equatable {
        // 供应商
        let provider: String
        // 名称
        let name: String
        // 套餐
        let plan: String?
        // 窗口列表
        let windows: [Window]
        // 描述
        let description: String
    }
    // 额度：观测时间与供应商列表
    struct Quota: Codable, Equatable {
        // 观测时间戳
        let observedAtUnix: Int64
        // 供应商列表
        let providers: [Provider]
        // 观测日期
        var observedDate: Date { Date(timeIntervalSince1970: TimeInterval(observedAtUnix)) }
    }
    // 代理明细行
    struct AgentRow: Codable, Equatable, Identifiable {
        // 标识
        let id: String
        // 名称
        let name: String
        // 套餐
        let plan: String?
        // 文本
        let text: String
        // 紧凑写法
        let compact: String
        // 精确写法
        let exact: String?
        // 读屏文案
        let accessibilityLabel: String
    }
    // 以下为顶层展示字段：Hooks 文案、Token、额度与代理明细
    let hookCountText: String
    // Hook 文本
    let hooksText: String
    // Hook 摘要
    let hooksSummary: String
    // 首个 Hook
    let hookLead: String?
    // 用量
    let tokens: Tokens?
    // 用量备用文案
    let tokenFallback: String
    // 用量读屏文案
    let tokenAccessibilityLabel: String
    // 额度
    let quota: Quota?
    // 代理明细行
    let agents: [AgentRow]
    // 无明细时的占位文案
    let agentsEmptyText: String

    // 由 Rust 生成的各类描述文案
    struct Labels: Decodable {
        // 锁屏内联的空态文案
        let widgetEmptyInline: String
        // 圆形空态
        let widgetEmptyCircular: String
        // 空态标题
        let widgetEmptyTitle: String
        // 空态帮助
        let widgetEmptyHelp: String
        // 同步文案
        let appleSyncText: String
        // 同步描述
        let appleSyncDescription: String
        // 紧凑同步文案
        let appleCompactSyncText: String
        // 紧凑同步描述（读屏用）
        let appleCompactSyncDescription: String
        // 额度更新说明
        let quotaUpdate: String
        // 紧凑的额度更新说明
        let quotaCompactUpdate: String
        // 被隐藏代理的提示
        let remainingAgents: String
        // 各供应商的描述
        let providerDescriptions: [String: String]
        // 锁屏内联文案
        let appleAccessoryInline: String
        // 锁屏圆形文案
        let appleAccessoryCircular: String
    }

    /// 无缓存时也读取共享空态投影，系统视图不另写一套缺失状态规则。
    // 没有缓存时也读取共享的空态投影，系统视图不另写一套缺失状态的规则
    static func emptyLabels(appName: String = "诡秘神谕") -> Labels? {
        // 请求空态文案，失败则为 nil
        try? LokiCore.decode(Labels.self, ["op": "view.describe", "view": NSNull(), "appName": appName])
    }

    /// 日期由 Foundation 按当前区域格式化，完整描述仍由同一 Rust 模板产生。
    // 日期由 Foundation 按当前区域格式化，完整描述仍由同一套 Rust 模板产生
    func labels(syncTime: String? = nil, compactSyncTime: String? = nil, quotaTime: String? = nil,
                // 额度紧凑时间与可见代理数
                quotaCompactTime: String? = nil, visibleAgents: Int? = nil) throws -> Labels {
        // 请求文案，失败则抛出
        guard let labels = try LokiCore.decode(Labels.self, ["op": "view.describe", "view": LokiCore.object(self),
            // 同步时间与紧凑同步时间
            "syncTime": syncTime as Any? ?? NSNull(), "compactSyncTime": compactSyncTime as Any? ?? NSNull(),
            // 额度时间与额度紧凑时间
            "quotaTime": quotaTime as Any? ?? NSNull(), "quotaCompactTime": quotaCompactTime as Any? ?? NSNull(),
            // 可见代理数，失败则抛出
            "visibleAgents": visibleAgents as Any? ?? NSNull()]) else { throw LokiCore.Failure(description: "展示描述无效") }
        // 返回文案
        return labels
    }
}

/// App 与组件共用的毫秒时间戳换算。
// App 与组件共用的毫秒时间戳换算
extension Date {
    // 由毫秒时间戳构造日期
    init(epochMs: Int64) { self.init(timeIntervalSince1970: Double(epochMs) / 1000) }
    // 转为毫秒时间戳
    var epochMs: Int64 { Int64(timeIntervalSince1970 * 1000) }
}

/// 组件的中文同步时间格式；静态保存，避免视图每次渲染重建 Locale。
// 组件的中文同步时间格式；静态保存，避免视图每次渲染都重建 Locale
enum SyncTimeFormat {
    // 中文区域设置
    static let locale = Locale(identifier: "zh_CN")
    /// 读屏使用的完整年月日时分。
    // 读屏使用的完整年月日时分
    static let full = Date.FormatStyle.dateTime.locale(locale).year().month().day().hour().minute()
    /// 空间有限时省略年份。
    // 空间有限时省略年份
    static let short = Date.FormatStyle.dateTime.locale(locale).month().day().hour().minute()
}
