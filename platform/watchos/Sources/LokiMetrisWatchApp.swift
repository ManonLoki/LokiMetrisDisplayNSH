import SwiftUI

@main
struct LokiMetrisWatchApp: App {
    @StateObject private var model: WatchModel
    @Environment(\.scenePhase) private var phase

    init() {
        // Eager process initialization also runs when watchOS launches us only for delivery.
        let runtime = WatchRuntime.shared
        _model = StateObject(wrappedValue: WatchModel(runtime: runtime))
    }

    var body: some Scene {
        WindowGroup {
            ContentView(model: model)
                .tint(Color(red: 103 / 255, green: 232 / 255, blue: 249 / 255))
                .onChange(of: phase) { _, phase in
                    if phase == .active { model.foreground() }
                }
        }
        .backgroundTask(.watchConnectivity) {
            await WatchRuntime.shared.backgroundDelivery()
        }
    }
}
