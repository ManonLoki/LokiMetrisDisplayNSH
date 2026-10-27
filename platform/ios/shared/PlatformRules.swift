import Foundation

/// Typed system-shell access to the shared Rust policies.
enum PlatformRules {
    static func call(_ operation: String, _ fields: [String: Any] = [:]) throws -> Any {
        var request = fields; request["op"] = "platform." + operation
        return try LokiCore.call(request)
    }
    static func dictionary(_ operation: String, _ fields: [String: Any] = [:]) -> [String: Any]? {
        (try? call(operation, fields)) as? [String: Any]
    }
}
