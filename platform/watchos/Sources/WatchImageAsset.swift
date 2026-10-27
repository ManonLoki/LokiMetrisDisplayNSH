import CoreGraphics
import Foundation
import ImageIO

/// 沿用旧 Watch 的 2 MiB、256px、512 帧预算；保留全部动画帧和原始时序。
struct WatchImageAsset: @unchecked Sendable {
    enum Failure: Error { case invalidImage, resourceLimit }
    let id: String
    let frames: [CGImage]
    /// 0 为无限循环，正数为总播放次数。
    let playCount: Int
    let decodedByteCount: Int
    private let frameEnds: [TimeInterval]
    let duration: TimeInterval
    var isAnimated: Bool { frames.count > 1 }

    static func decode(contentsOf url: URL) throws -> Self {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard url.isFileURL, values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, (1...32 * 1024 * 1024).contains(size) else { throw Failure.resourceLimit }
        try Task.checkCancellation()
        let data = try Data(contentsOf: url, options: .mappedIfSafe)
        guard data.count == size,
              let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              CGImageSourceGetStatus(source) == .statusComplete else { throw Failure.invalidImage }
        let count = CGImageSourceGetCount(source)
        guard (1...512).contains(count) else { throw Failure.resourceLimit }
        let metadata = CGImageSourceCopyProperties(source, nil) as? [CFString: Any] ?? [:]
        if let width = metadata[kCGImagePropertyPixelWidth] as? NSNumber,
           let height = metadata[kCGImagePropertyPixelHeight] as? NSNumber {
            guard validDimensions(width.doubleValue, height.doubleValue) else { throw Failure.resourceLimit }
        }
        let maxDecodedBytes = 2 * 1024 * 1024
        let perFrameBytes = maxDecodedBytes / count
        var edge = min(256, Int(sqrt(Double(perFrameBytes / 4))))
        guard edge > 0 else { throw Failure.resourceLimit }
        var images: [CGImage] = [], ends: [TimeInterval] = []
        var bytes = 0, total: TimeInterval = 0
        for index in 0..<count {
            try Task.checkCancellation()
            try autoreleasepool {
                guard let properties = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any],
                      let width = properties[kCGImagePropertyPixelWidth] as? NSNumber,
                      let height = properties[kCGImagePropertyPixelHeight] as? NSNumber,
                      validDimensions(width.doubleValue, height.doubleValue) else { throw Failure.resourceLimit }
                var frame = try thumbnail(source, index: index, edge: edge)
                // ImageIO 行字节含对齐开销，按实际占用缩小，避免误拒绝可缩放的动画。
                while frame.bytesPerRow * frame.height > perFrameBytes {
                    guard edge > 1 else { throw Failure.resourceLimit }
                    edge = max(1, min(edge - 1, Int(Double(edge) * sqrt(Double(perFrameBytes) / Double(frame.bytesPerRow * frame.height)))))
                    frame = try thumbnail(source, index: index, edge: edge)
                }
                let size = frame.bytesPerRow * frame.height
                guard size <= maxDecodedBytes - bytes else { throw Failure.resourceLimit }
                total += delayTime(properties)
                guard total.isFinite else { throw Failure.invalidImage }
                bytes += size
                images.append(frame)
                ends.append(total)
            }
        }
        return Self(id: url.absoluteString, frames: images, playCount: plays(metadata),
                    decodedByteCount: bytes, frameEnds: ends, duration: total)
    }

    private static func validDimensions(_ width: Double, _ height: Double) -> Bool {
        width.isFinite && height.isFinite && width > 0 && height > 0 && width * height <= 16_777_216
    }

    private static func thumbnail(_ source: CGImageSource, index: Int, edge: Int) throws -> CGImage {
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, index, [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: edge,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceShouldCache: false,
        ] as CFDictionary) else { throw Failure.invalidImage }
        return image
    }

    func frameIndex(at elapsed: TimeInterval) -> Int {
        guard isAnimated, elapsed.isFinite, elapsed > 0, duration > 0 else { return 0 }
        if playCount > 0, elapsed / duration >= Double(playCount) { return frames.count - 1 }
        let time = elapsed.truncatingRemainder(dividingBy: duration)
        return frameEnds.firstIndex(where: { time < $0 }) ?? frames.count - 1
    }

    func timeUntilNextFrame(at elapsed: TimeInterval) -> TimeInterval? {
        guard isAnimated, duration > 0 else { return nil }
        let elapsed = elapsed.isFinite ? max(0, elapsed) : 0
        if playCount > 0, elapsed / duration >= Double(playCount) { return nil }
        let time = elapsed.truncatingRemainder(dividingBy: duration)
        return (frameEnds.first(where: { time < $0 }) ?? duration) - time
    }

    private static func delayTime(_ properties: [CFString: Any]) -> TimeInterval {
        for (dictionary, unclamped, clamped) in [
            (kCGImagePropertyGIFDictionary, kCGImagePropertyGIFUnclampedDelayTime, kCGImagePropertyGIFDelayTime),
            (kCGImagePropertyPNGDictionary, kCGImagePropertyAPNGUnclampedDelayTime, kCGImagePropertyAPNGDelayTime),
            (kCGImagePropertyWebPDictionary, kCGImagePropertyWebPUnclampedDelayTime, kCGImagePropertyWebPDelayTime),
        ] {
            if let values = properties[dictionary] as? [CFString: Any] {
                let value = (values[unclamped] as? NSNumber)?.doubleValue ?? (values[clamped] as? NSNumber)?.doubleValue ?? 0.1
                return value.isFinite && value > 0 ? value : 0.1
            }
        }
        return 0.1
    }

    private static func plays(_ metadata: [CFString: Any]) -> Int {
        for (dictionary, key) in [
            (kCGImagePropertyGIFDictionary, kCGImagePropertyGIFLoopCount),
            (kCGImagePropertyPNGDictionary, kCGImagePropertyAPNGLoopCount),
            (kCGImagePropertyWebPDictionary, kCGImagePropertyWebPLoopCount),
        ] {
            if let values = metadata[dictionary] as? [CFString: Any], let count = values[key] as? NSNumber {
                let value = count.intValue
                guard value > 0 else { return 0 }
                // GIF 的正数记录首次之后的重复次数，APNG/WebP 直接记录总次数。
                return dictionary == kCGImagePropertyGIFDictionary && value < Int.max ? value + 1 : value
            }
        }
        return 1
    }
}

/// 暂停保留已播放时长，恢复后继续；不使用可被系统校准的墙钟。
struct WatchImagePlaybackClock {
    private(set) var assetID: String?
    private var accumulated: TimeInterval = 0
    private var started: ContinuousClock.Instant?

    func elapsed(at instant: ContinuousClock.Instant) -> TimeInterval {
        guard let started else { return accumulated }
        let interval = started.duration(to: instant).components
        return accumulated + max(0, Double(interval.seconds) + Double(interval.attoseconds) / 1e18)
    }

    mutating func update(assetID: String, playing: Bool, at instant: ContinuousClock.Instant) {
        if self.assetID != assetID {
            self.assetID = assetID
            accumulated = 0
            started = nil
        }
        setPlaying(playing, at: instant)
    }

    mutating func setPlaying(_ playing: Bool, at instant: ContinuousClock.Instant) {
        if playing {
            if started == nil { started = instant }
        } else {
            accumulated = elapsed(at: instant)
            started = nil
        }
    }
}
