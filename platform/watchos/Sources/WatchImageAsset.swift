// 引入 CoreGraphics、Foundation 与 ImageIO
import CoreGraphics
// 基础库
import Foundation
// 图像解码库
import ImageIO

/// 沿用旧 Watch 的 2 MiB、256px、512 帧预算；保留全部动画帧和原始时序。
// 沿用旧版手表的预算：2 MiB 解码内存、256 像素、512 帧；保留全部动画帧及原始时序
struct WatchImageAsset: @unchecked Sendable {
    // 无效图片、超出资源限制
    enum Failure: Error { case invalidImage, resourceLimit }
    // 资源标识
    let id: String
    // 解码后的各帧
    let frames: [CGImage]
    /// 0 为无限循环，正数为总播放次数。
    // 播放次数：0 无限循环，正数为总次数
    let playCount: Int
    // 解码后总字节数
    let decodedByteCount: Int
    // 各帧的结束时刻（累计）
    private let frameEnds: [TimeInterval]
    // 单轮总时长
    let duration: TimeInterval
    // 多于一帧才算动画
    var isAnimated: Bool { frames.count > 1 }

    // 解码：先检查文件，再逐帧生成缩略图并累计内存与时长
    static func decode(contentsOf url: URL) throws -> Self {
        // 必须是本地普通文件、大小在范围内
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        // 须是本地普通文件且不是链接
        guard url.isFileURL, values.isRegularFile == true, values.isSymbolicLink != true,
              // 大小在 1 字节到 32 MiB
              let size = values.fileSize, (1...32 * 1024 * 1024).contains(size) else { throw Failure.resourceLimit }
        // 检查任务是否被取消
        try Task.checkCancellation()
        // 以内存映射方式读取
        let data = try Data(contentsOf: url, options: .mappedIfSafe)
        // 内容完整、可被 ImageIO 识别
        guard data.count == size,
              // 创建图像源
              let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              // 状态必须完整
              CGImageSourceGetStatus(source) == .statusComplete else { throw Failure.invalidImage }
        // 帧数 1 到 512
        let count = CGImageSourceGetCount(source)
        // 帧数在 1 到 512
        guard (1...512).contains(count) else { throw Failure.resourceLimit }
        // 整体元数据
        let metadata = CGImageSourceCopyProperties(source, nil) as? [CFString: Any] ?? [:]
        // 画布像素数不超过 1600 万
        if let width = metadata[kCGImagePropertyPixelWidth] as? NSNumber,
           // 高度
           let height = metadata[kCGImagePropertyPixelHeight] as? NSNumber {
            // 尺寸须合法，否则抛出
            guard validDimensions(width.doubleValue, height.doubleValue) else { throw Failure.resourceLimit }
        }
        // 解码总内存预算
        let maxDecodedBytes = 2 * 1024 * 1024
        // 平均每帧的预算
        let perFrameBytes = maxDecodedBytes / count
        // 缩略图边长：不超过 256，并按预算估算
        var edge = min(256, Int(sqrt(Double(perFrameBytes / 4))))
        // 边长必须大于 0
        guard edge > 0 else { throw Failure.resourceLimit }
        // 图片与各帧结束时间
        var images: [CGImage] = [], ends: [TimeInterval] = []
        // 累计字节数与总时长
        var bytes = 0, total: TimeInterval = 0
        // 逐帧解码
        for index in 0..<count {
            // 检查任务是否被取消
            try Task.checkCancellation()
            // 放进自动释放池
            try autoreleasepool {
                // 每帧尺寸也要合法
                guard let properties = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any],
                      // 宽度
                      let width = properties[kCGImagePropertyPixelWidth] as? NSNumber,
                      // 高度
                      let height = properties[kCGImagePropertyPixelHeight] as? NSNumber,
                      // 尺寸须合法
                      validDimensions(width.doubleValue, height.doubleValue) else { throw Failure.resourceLimit }
                // 取缩略图
                var frame = try thumbnail(source, index: index, edge: edge)
                // ImageIO 行字节含对齐开销，按实际占用缩小，避免误拒绝可缩放的动画。
                // ImageIO 的行字节含对齐开销，按实际占用缩小，避免误拒绝可缩放的动画
                while frame.bytesPerRow * frame.height > perFrameBytes {
                    // 边长已降到最小则抛出资源超限
                    guard edge > 1 else { throw Failure.resourceLimit }
                    // 按每帧字节预算缩小边长
                    edge = max(1, min(edge - 1, Int(Double(edge) * sqrt(Double(perFrameBytes) / Double(frame.bytesPerRow * frame.height)))))
                    // 取缩略图
                    frame = try thumbnail(source, index: index, edge: edge)
                }
                // 累计内存不得超过预算
                let size = frame.bytesPerRow * frame.height
                // 累计字节数不得超过上限
                guard size <= maxDecodedBytes - bytes else { throw Failure.resourceLimit }
                // 累计时长
                total += delayTime(properties)
                // 总时长须为有限数
                guard total.isFinite else { throw Failure.invalidImage }
                // 累计字节数
                bytes += size
                // 加入帧
                images.append(frame)
                // 加入该帧结束时间
                ends.append(total)
            }
        }
        // 构造资源
        return Self(id: url.absoluteString, frames: images, playCount: plays(metadata),
                    // 解码字节数、各帧结束时间与总时长
                    decodedByteCount: bytes, frameEnds: ends, duration: total)
    }

    // 尺寸合法：有限、为正且像素总数不超过 16,777,216
    private static func validDimensions(_ width: Double, _ height: Double) -> Bool {
        // 宽高有限、为正且面积不超限
        width.isFinite && height.isFinite && width > 0 && height > 0 && width * height <= 16_777_216
    }

    // 生成缩略图：按需转换方向并立即缓存解码结果
    private static func thumbnail(_ source: CGImageSource, index: Int, edge: Int) throws -> CGImage {
        // 创建指定帧的缩略图
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, index, [
            // 总是创建缩略图
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            // 应用方向变换
            kCGImageSourceCreateThumbnailWithTransform: true,
            // 最大边长
            kCGImageSourceThumbnailMaxPixelSize: edge,
            // 立即解码缓存
            kCGImageSourceShouldCacheImmediately: true,
            // 不缓存
            kCGImageSourceShouldCache: false,
        // 失败则抛出图片无效
        ] as CFDictionary) else { throw Failure.invalidImage }
        // 返回解码出的图片
        return image
    }

    // 给定已播放时长，返回应显示的帧：播完指定次数则停在最后一帧
    func frameIndex(at elapsed: TimeInterval) -> Int {
        // 非动图或时间无效则从第 0 帧开始
        guard isAnimated, elapsed.isFinite, elapsed > 0, duration > 0 else { return 0 }
        // 播放次数用完则停在最后一帧
        if playCount > 0, elapsed / duration >= Double(playCount) { return frames.count - 1 }
        // 在一轮内的时间
        let time = elapsed.truncatingRemainder(dividingBy: duration)
        // 找到第一个结束时间大于当前时间的帧
        return frameEnds.firstIndex(where: { time < $0 }) ?? frames.count - 1
    }

    // 距离下一帧还要多久；静态或已播完返回 nil
    func timeUntilNextFrame(at elapsed: TimeInterval) -> TimeInterval? {
        // 非动图则没有下一次切换
        guard isAnimated, duration > 0 else { return nil }
        // 已经过的时间，无效按 0
        let elapsed = elapsed.isFinite ? max(0, elapsed) : 0
        // 播放次数用完则没有下一次
        if playCount > 0, elapsed / duration >= Double(playCount) { return nil }
        // 在一轮内的时间
        let time = elapsed.truncatingRemainder(dividingBy: duration)
        // 到下一帧的剩余时间
        return (frameEnds.first(where: { time < $0 }) ?? duration) - time
    }

    // 帧延迟：依次看 GIF、APNG、WebP 的字典，优先未限幅的值，缺失或非法时取 0.1 秒
    private static func delayTime(_ properties: [CFString: Any]) -> TimeInterval {
        // 依次尝试各属性字典
        for (dictionary, unclamped, clamped) in [
            // GIF 的延迟属性
            (kCGImagePropertyGIFDictionary, kCGImagePropertyGIFUnclampedDelayTime, kCGImagePropertyGIFDelayTime),
            // APNG 的延迟属性
            (kCGImagePropertyPNGDictionary, kCGImagePropertyAPNGUnclampedDelayTime, kCGImagePropertyAPNGDelayTime),
            // WebP 的延迟属性
            (kCGImagePropertyWebPDictionary, kCGImagePropertyWebPUnclampedDelayTime, kCGImagePropertyWebPDelayTime),
        // 逐项尝试
        ] {
            // 取到字典
            if let values = properties[dictionary] as? [CFString: Any] {
                // 优先取未限制值，其次取限制值
                let value = (values[unclamped] as? NSNumber)?.doubleValue ?? (values[clamped] as? NSNumber)?.doubleValue ?? 0.1
                // 有限且为正才采用，否则取默认值
                return value.isFinite && value > 0 ? value : 0.1
            }
        }
        // 缺失时使用默认帧间隔
        return 0.1
    }

    // 播放次数：GIF 的正数记录首次之后的重复次数，APNG/WebP 直接记录总次数；没有记录则播放一次
    private static func plays(_ metadata: [CFString: Any]) -> Int {
        // 依次尝试各元数据字典
        for (dictionary, key) in [
            // GIF 的循环次数
            (kCGImagePropertyGIFDictionary, kCGImagePropertyGIFLoopCount),
            // APNG 的循环次数
            (kCGImagePropertyPNGDictionary, kCGImagePropertyAPNGLoopCount),
            // WebP 的循环次数
            (kCGImagePropertyWebPDictionary, kCGImagePropertyWebPLoopCount),
        // 逐项尝试
        ] {
            // 取循环次数
            if let values = metadata[dictionary] as? [CFString: Any], let count = values[key] as? NSNumber {
                // 转为整数
                let value = count.intValue
                // 0 表示无限
                guard value > 0 else { return 0 }
                // GIF 的正数记录首次之后的重复次数，APNG/WebP 直接记录总次数。
                return dictionary == kCGImagePropertyGIFDictionary && value < Int.max ? value + 1 : value
            }
        }
        // 默认播放一次
        return 1
    }
}

/// 暂停保留已播放时长，恢复后继续；不使用可被系统校准的墙钟。
// 播放时钟：暂停保留已播放时长，恢复后继续；不使用可被系统校准的墙钟
struct WatchImagePlaybackClock {
    // 当前图片
    private(set) var assetID: String?
    // 已累计的播放时长
    private var accumulated: TimeInterval = 0
    // 本段开始播放的时刻
    private var started: ContinuousClock.Instant?

    // 总播放时长 = 已累计 + 本段已播
    func elapsed(at instant: ContinuousClock.Instant) -> TimeInterval {
        // 未开始则返回已累计时间
        guard let started else { return accumulated }
        // 自开始起的间隔
        let interval = started.duration(to: instant).components
        // 已累计时间加上本次间隔
        return accumulated + max(0, Double(interval.seconds) + Double(interval.attoseconds) / 1e18)
    }

    // 换图片则清零；再按是否播放更新
    mutating func update(assetID: String, playing: Bool, at instant: ContinuousClock.Instant) {
        // 资源标识变了
        if self.assetID != assetID {
            // 记录新标识
            self.assetID = assetID
            // 累计时间清零
            accumulated = 0
            // 清除开始时刻
            started = nil
        }
        // 设置播放状态
        setPlaying(playing, at: instant)
    }

    // 开始：记录起点；暂停：把本段计入累计
    mutating func setPlaying(_ playing: Bool, at instant: ContinuousClock.Instant) {
        // 开始播放
        if playing {
            // 记下开始时刻
            if started == nil { started = instant }
        // 停止播放
        } else {
            // 累计已播放时间
            accumulated = elapsed(at: instant)
            // 清除开始时刻
            started = nil
        }
    }
}
