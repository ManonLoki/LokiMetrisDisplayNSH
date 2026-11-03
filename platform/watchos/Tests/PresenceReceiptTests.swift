import Foundation

/// 纯状态使用同一连续时钟的可控Instant，不启动WatchConnectivity或读真实UserDefaults。
@main
struct PresenceReceiptTests {
    static func main() throws {
        let start = ContinuousClock().now
        let stream = "7b9dd6ed-9be7-4e86-9f07-8f24ae634301"
        let source = "39acf859-ccec-4e4e-8d87-52d5473ff623"
        let info = WatchPresenceInfo(deviceId: "7e6db3af-3449-4e61-9efb-cc1378c7a012", name: "Watch", appVersion: "1")
        let probe = WatchPresenceProbe(version: 1, type: "presenceProbe", challenge: UUID().uuidString.lowercased(), streamId: stream, revision: "1", sourceId: source)
        let bytes = try JSONSerialization.data(withJSONObject: ["version": 1, "streamId": stream, "revision": "1", "sourceId": source,
                                                               "receivedAtMs": 1, "snapshot": ["version": 1, "tokenError": NSNull(), "tokens": NSNull(),
                                                               "hooksAvailable": false, "hooks": [], "providerQuota": NSNull(), "enabledAgents": []], "assets": []])
        let manifest = try JSONDecoder().decode(CompanionManifest.self, from: bytes)
        try manifest.validate()
        var state = WatchForegroundPresence()
        state.adoptedChanged(manifest, receiptForegroundId: state.foregroundId, at: start)
        assert(!state.reply(probe, info: info, at: start).active)
        assert(state.reply(probe, info: info, at: start).sync == nil, "后台接纳不构成同步")
        assert(state.setActive(true) == nil)
        let first = state.foregroundId!
        assert(state.reply(probe, info: info, at: start).sync == nil, "前台读取旧缓存不构成新同步")
        state.adoptedChanged(manifest, receiptForegroundId: state.foregroundId, at: start)
        assert(state.reply(probe, info: info, at: start.advanced(by: .microseconds(1))).sync?.ageMs == 1)
        state.adoptedChanged(manifest, receiptForegroundId: first, at: start.advanced(by: .seconds(1)))
        assert(state.reply(probe, info: info, at: start.advanced(by: .seconds(2))).sync?.ageMs == 2000, "同revision不刷新原接纳时刻")
        let mismatch = WatchPresenceProbe(version: 1, type: "presenceProbe", challenge: probe.challenge, streamId: stream, revision: "2", sourceId: source)
        assert(state.reply(mismatch, info: info, at: start).sync == nil, "不同快照只能报告真实active，不能报告同步")
        assert(state.reply(probe, info: info, at: start.advanced(by: .seconds(7 * 24 * 60 * 60 + 1))).sync == nil)
        assert(state.setActive(false) == first)
        assert(!state.reply(probe, info: info, at: start).active)
        assert(state.reply(probe, info: info, at: start).sync == nil)
        assert(state.setActive(false) == nil)
        _ = state.setActive(true)
        assert(state.foregroundId != first)
        state.adoptedChanged(manifest, receiptForegroundId: first, at: start)
        assert(state.reply(probe, info: info, at: start).sync == nil, "旧前台排队的changed投递不能在新前台取得同步资格")
        state.adoptedChanged(manifest, receiptForegroundId: nil, at: start)
        assert(state.reply(probe, info: info, at: start).sync == nil, "后台到达的changed投递不能在新前台取得同步资格")
        let gate = WatchForegroundReceiptGate()
        assert(gate.capture() == nil)
        gate.set(first)
        let captured = gate.capture()
        gate.set(nil)
        assert(captured == first && gate.capture() == nil)
        gate.set(state.foregroundId)
        assert(gate.capture() != captured)
        let encoded = try JSONEncoder().encode(probe)
        _ = try WatchPresenceProbe.decode(encoded)
        var extra = try JSONSerialization.jsonObject(with: encoded) as! [String: Any]
        extra["cached"] = true
        do { _ = try WatchPresenceProbe.decode(JSONSerialization.data(withJSONObject: extra)); assertionFailure("未知字段应拒绝") } catch {}
        extra.removeValue(forKey: "cached"); extra["revision"] = "01"
        do { _ = try WatchPresenceProbe.decode(JSONSerialization.data(withJSONObject: extra)); assertionFailure("非规范revision应拒绝") } catch {}
        let inactive = WatchForegroundPresence().reply(probe, info: info, at: start)
        let inactiveObject = try JSONSerialization.jsonObject(with: JSONEncoder().encode(inactive)) as! [String: Any]
        assert(inactiveObject["foregroundId"] is NSNull && inactiveObject["sync"] is NSNull)
        print("Watch foreground presence tests passed")
    }
}
