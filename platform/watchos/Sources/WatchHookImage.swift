import SwiftUI

/// URL 已由缓存校验；解码在独立任务完成，界面只持有有界缩略图。
struct WatchHookImage: View {
    let url: URL
    var playbackEnabled = true
    @State private var asset: WatchImageAsset?
    @State private var failed = false

    var body: some View {
        Group {
            if let asset, asset.id == url.absoluteString {
                WatchImagePlayback(asset: asset, playbackEnabled: playbackEnabled)
            } else {
                VStack(spacing: 4) {
                    Image(systemName: "photo")
                    Text(failed ? "图片无法显示" : "正在读取图片").font(.caption2)
                }
                .foregroundStyle(.secondary)
            }
        }
        .task(id: url) {
            guard asset?.id != url.absoluteString else { return }
            asset = nil
            failed = false
            let source = url
            let loading = Task.detached(priority: .utility) { try WatchImageAsset.decode(contentsOf: source) }
            defer { loading.cancel() }
            do {
                let decoded = try await withTaskCancellationHandler {
                    try await loading.value
                } onCancel: { loading.cancel() }
                guard !Task.isCancelled else { return }
                asset = decoded
            } catch {
                if !Task.isCancelled { failed = true }
            }
        }
        .accessibilityLabel("Hook 图片")
        .accessibilityIdentifier("watch-hook-image")
    }
}

/// 只累计当前可见卡片的播放时长，离页、后台和常亮模式停止定时唤醒。
private struct WatchImagePlayback: View {
    let asset: WatchImageAsset
    let playbackEnabled: Bool
    @State private var playback = WatchImagePlaybackClock()
    @State private var displayedFrame = 0
    @State private var visible = false
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.isLuminanceReduced) private var reducedLuminance

    private var isPlaying: Bool {
        visible && asset.isAnimated && playbackEnabled && !reduceMotion && !reducedLuminance && scenePhase == .active
    }

    private struct PlaybackTaskID: Equatable {
        let assetID: String
        let playing: Bool
    }

    var body: some View {
        let index = reduceMotion || playback.assetID != asset.id ? 0 : displayedFrame
        Image(decorative: asset.frames[min(index, asset.frames.count - 1)], scale: 1)
            .resizable().scaledToFit()
            .task(id: PlaybackTaskID(assetID: asset.id, playing: isPlaying)) { await playFrames() }
            .onAppear { updateVisibility(true) }
            .onDisappear { updateVisibility(false) }
            .onChange(of: isPlaying) { _, _ in synchronizePlayback() }
            .onChange(of: asset.id) { _, _ in
                synchronizePlayback()
                displayedFrame = 0
            }
    }

    private func updateVisibility(_ value: Bool) {
        visible = value
        synchronizePlayback()
    }

    private func synchronizePlayback() {
        playback.update(assetID: asset.id, playing: isPlaying, at: .now)
    }

    @MainActor
    private func playFrames() async {
        guard !Task.isCancelled else { return }
        synchronizePlayback()
        let clock = ContinuousClock()
        while !Task.isCancelled {
            let elapsed = playback.elapsed(at: clock.now)
            displayedFrame = asset.frameIndex(at: elapsed)
            guard isPlaying, let interval = asset.timeUntilNextFrame(at: elapsed) else {
                playback.setPlaying(false, at: clock.now)
                return
            }
            do {
                try await clock.sleep(for: .seconds(max(1.0 / 60, interval)), tolerance: .milliseconds(2))
            } catch { return }
        }
    }
}
