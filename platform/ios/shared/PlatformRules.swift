// 引入 Foundation
import Foundation

/// 以类型化方式让系统外壳访问共享的 Rust 策略。
// 系统外壳访问 Rust 共享策略的入口
enum PlatformRules {
    // 调用 platform.<操作> 并返回结果
    static func call(_ operation: String, _ fields: [String: Any] = [:]) throws -> Any {
        // 在字段上加操作名，转交 Rust 核心
        var request = fields; request["op"] = "platform." + operation
        // 调用展示核心
        return try LokiCore.call(request)
    }
    // 调用并把结果当作字典，失败返回 nil
    static func dictionary(_ operation: String, _ fields: [String: Any] = [:]) -> [String: Any]? {
        // 调用规则并转为字典
        (try? call(operation, fields)) as? [String: Any]
    }
}
