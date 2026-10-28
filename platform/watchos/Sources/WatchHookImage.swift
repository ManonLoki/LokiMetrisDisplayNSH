// 引入 SwiftUI
import SwiftUI

/// URL 已由缓存校验；解码在独立任务完成，界面只持有有界缩略图。
// Hook 图片：地址已由缓存校验；解码在独立任务完成，界面只持有有界的缩略图
struct WatchHookImage: View {
    // 图片地址
    let url: URL
    // 是否允许播放动图
    var playbackEnabled = true
    // 解码结果
    @State private var asset: WatchImageAsset?
    // 是否加载失败
    @State private var failed = false

    // 视图主体
    var body: some View {
        // 按图片状态分组显示
        Group {
            // 已解码且与当前地址一致：显示图片（动图则播放）
            if let asset, asset.id == url.absoluteString {
                // 按资源播放图片
                WatchImagePlayback(asset: asset, playbackEnabled: playbackEnabled)
            // 否则显示“正在读取”或“无法显示”
            } else {
                // 无图时的占位
                VStack(spacing: 4) {
                    // 占位图标
                    Image(systemName: "photo")
                    // 加载失败或正在读取的提示
                    Text(failed ? "图片无法显示" : "正在读取图片").font(.caption2)
                }
                // 次要颜色
                .foregroundStyle(.secondary)
            }
        }
        // 地址变化时重新解码
        .task(id: url) {
            // 资源已是该地址则不重复加载
            guard asset?.id != url.absoluteString else { return }
            // 清空旧资源
            asset = nil
            // 清除失败标记
            failed = false
            // 记下当前地址
            let source = url
            // 在后台线程解码
            let loading = Task.detached(priority: .utility) { try WatchImageAsset.decode(contentsOf: source) }
            // 结束时取消加载
            defer { loading.cancel() }
            // 尝试加载
            do {
                // 取消时同步取消解码任务
                let decoded = try await withTaskCancellationHandler {
                    // 等待加载结果
                    try await loading.value
                // 取消时取消加载
                } onCancel: { loading.cancel() }
                // 已被取消则返回
                guard !Task.isCancelled else { return }
                // 保存解码结果
                asset = decoded
            // 出错
            } catch {
                // 未取消才标记失败
                if !Task.isCancelled { failed = true }
            }
        }
        // 读屏标签
        .accessibilityLabel("Hook 图片")
        // 测试标识
        .accessibilityIdentifier("watch-hook-image")
    }
}

/// 只累计当前可见卡片的播放时长，离页、后台和常亮模式停止定时唤醒。
// 播放视图：只累计当前可见卡片的播放时长，离页、后台和常亮模式停止定时唤醒
private struct WatchImagePlayback: View {
    // 图片资源
    let asset: WatchImageAsset
    // 是否允许播放
    let playbackEnabled: Bool
    // 播放时钟
    @State private var playback = WatchImagePlaybackClock()
    // 当前显示的帧
    @State private var displayedFrame = 0
    // 视图是否可见
    @State private var visible = false
    // 环境：场景阶段、减弱动态效果、低亮度
    @Environment(\.scenePhase) private var scenePhase
    // 读取“减少动态效果”设置
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    // 读取“降低亮度”状态
    @Environment(\.isLuminanceReduced) private var reducedLuminance

    // 可见、是动图、允许播放、未减弱动态、未低亮度且在前台才播放
    private var isPlaying: Bool {
        // 可见、是动图、允许播放且未减少动效、未降亮度
        visible && asset.isAnimated && playbackEnabled && !reduceMotion && !reducedLuminance && scenePhase == .active
    }

    // 任务标识：图片或播放状态变化时重启播放任务
    private struct PlaybackTaskID: Equatable {
        // 资源标识
        let assetID: String
        // 是否正在播放
        let playing: Bool
    }

    // 视图主体
    var body: some View {
        // 减弱动态效果时只显示首帧
        let index = reduceMotion || playback.assetID != asset.id ? 0 : displayedFrame
        // 显示当前帧，下标不越界
        Image(decorative: asset.frames[min(index, asset.frames.count - 1)], scale: 1)
            // 可缩放并适应容器
            .resizable().scaledToFit()
            // 按资源与播放状态启动任务
            .task(id: PlaybackTaskID(assetID: asset.id, playing: isPlaying)) { await playFrames() }
            // 出现时标记可见
            .onAppear { updateVisibility(true) }
            // 消失时标记不可见
            .onDisappear { updateVisibility(false) }
            // 播放状态变化时同步时钟
            .onChange(of: isPlaying) { _, _ in synchronizePlayback() }
            // 换图片时同步并回到首帧
            .onChange(of: asset.id) { _, _ in
                // 同步播放状态
                synchronizePlayback()
                // 回到第 0 帧
                displayedFrame = 0
            }
    }

    // 更新可见性
    private func updateVisibility(_ value: Bool) {
        // 记下新状态
        visible = value
        // 同步播放状态
        synchronizePlayback()
    }

    // 把播放状态同步给时钟
    private func synchronizePlayback() {
        // 更新播放状态
        playback.update(assetID: asset.id, playing: isPlaying, at: .now)
    }

    // 在主线程执行
    @MainActor
    // 播放循环：按已播放时长选帧，睡到下一帧再继续；不播放时停下并暂停时钟
    private func playFrames() async {
        // 已取消则返回
        guard !Task.isCancelled else { return }
        // 同步播放状态
        synchronizePlayback()
        // 连续时钟
        let clock = ContinuousClock()
        // 任务未取消时循环
        while !Task.isCancelled {
            // 当前已播放时间
            let elapsed = playback.elapsed(at: clock.now)
            // 按已播放时间取帧
            displayedFrame = asset.frameIndex(at: elapsed)
            // 已停止播放或已播完，则暂停时钟并退出
            guard isPlaying, let interval = asset.timeUntilNextFrame(at: elapsed) else {
                // 停止播放
                playback.setPlaying(false, at: clock.now)
                // 返回
                return
            }
            // 尝试执行
            do {
                // 至少 1/60 秒，容忍 2 毫秒
                try await clock.sleep(for: .seconds(max(1.0 / 60, interval)), tolerance: .milliseconds(2))
            // 出错则返回
            } catch { return }
        }
    }
}
