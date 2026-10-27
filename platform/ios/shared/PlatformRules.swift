import Foundation

/// Typed system-shell access to the shared Rust policies. Wide clocks and counters
/// use decimal strings; Foundation's NSNumber/Double conversion never sees them.
enum PlatformRules {
    static func call(_ operation: String, _ fields: [String: Any] = [:]) throws -> Any {
        var request = fields; request["op"] = "platform." + operation
        return try LokiCore.call(request)
    }
    static func bool(_ operation: String, _ fields: [String: Any] = [:]) -> Bool {
        (try? call(operation, fields)) as? Bool == true
    }
    static func dictionary(_ operation: String, _ fields: [String: Any] = [:]) -> [String: Any]? {
        (try? call(operation, fields)) as? [String: Any]
    }
    static func optional<T>(_ value: T?) -> Any { value.map { $0 as Any } ?? NSNull() }
    static func object<T: Encodable>(_ value: T?) -> Any {
        value.flatMap { try? LokiCore.object($0) } ?? NSNull()
    }
    static func finite(_ value: Double) -> Any { value.isFinite ? value as Any : NSNull() }
    static func unsigned(_ value: Any?) -> UInt64? {
        if let value = value as? String { return UInt64(value) }
        return (value as? NSNumber)?.uint64Value
    }
}
