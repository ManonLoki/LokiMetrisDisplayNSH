import Foundation
import SwiftUI

@MainActor
final class WatchModel: ObservableObject {
    @Published private(set) var manifest: CompanionManifest?
    @Published private(set) var message: String?
    @Published private(set) var imageRevision = 0
    private let runtime: WatchRuntime
    private var observer: NSObjectProtocol?

    init(runtime: WatchRuntime) {
        self.runtime = runtime
        manifest = runtime.cache?.manifest
        message = runtime.startupMessage
        observer = NotificationCenter.default.addObserver(forName: .watchCacheChanged, object: nil, queue: .main) { [weak self] notification in
            let message = notification.userInfo?["error"] as? String
            Task { @MainActor [weak self] in
                guard let self else { return }
                self.manifest = self.runtime.cache?.manifest
                self.message = message
                self.imageRevision += 1
            }
        }
        // A delivery may have committed between the first read and observer registration.
        manifest = runtime.cache?.manifest
    }

    deinit {
        if let observer { NotificationCenter.default.removeObserver(observer) }
    }

    func foreground() {
        manifest = runtime.cache?.manifest
        imageRevision += 1
        runtime.transport?.requestLatest()
    }
    func imageURL(_ hash: String?) -> URL? {
        guard let hash else { return nil }
        return runtime.cache?.imageURL(hash: hash)
    }
}

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

