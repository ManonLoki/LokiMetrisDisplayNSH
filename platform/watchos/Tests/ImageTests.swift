// 引入 CoreGraphics、Foundation 与 ImageIO
import CoreGraphics
import Foundation
import ImageIO

// 图片测试入口
@main
struct ImageTests {
    static func main() throws {
        // 参数：PNG 夹具、临时目录、报告路径
        guard CommandLine.arguments.count == 4 else {
            throw NSError(domain: "ImageTests", code: 1,
                userInfo: [NSLocalizedDescriptionKey: "usage: image-tests <png-fixture> <temporary-directory> <report.json>"])
        }
        let fixture = URL(fileURLWithPath: CommandLine.arguments[1])
        let result = URL(fileURLWithPath: CommandLine.arguments[2], isDirectory: true)
        let reportURL = URL(fileURLWithPath: CommandLine.arguments[3])
        try FileManager.default.createDirectory(at: result, withIntermediateDirectories: true)
        // 静态 PNG：单帧、不超过 256 像素和 2 MiB，且没有下一帧
        let png = try WatchImageAsset.decode(contentsOf: fixture)
        precondition(png.frames.count == 1 && !png.isAnimated)
        precondition(png.frames[0].width <= 256 && png.frames[0].height <= 256)
        precondition(png.decodedByteCount <= 2 * 1024 * 1024 && png.timeUntilNextFrame(at: 0) == nil)
        // 用红、蓝两种纯色构造两帧 8x8 图像
        let colors: [[UInt8]] = [[255, 0, 0, 255], [0, 0, 255, 255]]
        let frames = colors.map { color -> CGImage in
            let bytes = Data((0..<64).flatMap { _ in color })
            let provider = CGDataProvider(data: bytes as CFData)!
            return CGImage(width: 8, height: 8, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: 32,
                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)!
        }
        // 写出只循环一次的 GIF，帧延迟 0.05 秒和 0.2 秒
        let gif = result.appendingPathComponent("finite.gif")
        let destination = CGImageDestinationCreateWithURL(gif as CFURL, "com.compuserve.gif" as CFString, 2, nil)!
        CGImageDestinationSetProperties(destination, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFLoopCount: 1]] as CFDictionary)
        for (index, frame) in frames.enumerated() {
            CGImageDestinationAddImage(destination, frame, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFDelayTime: index == 0 ? 0.05 : 0.2]] as CFDictionary)
        }
        precondition(CGImageDestinationFinalize(destination))
        // 解码后：两帧、是动画、总播放两次
        let asset = try WatchImageAsset.decode(contentsOf: gif)
        precondition(asset.frames.count == 2 && asset.isAnimated && asset.playCount == 2)
        // 单轮时长 0.25 秒
        precondition(abs(asset.duration - 0.25) < 0.001)
        // 按时间选帧
        precondition(asset.frameIndex(at: 0.01) == 0 && asset.frameIndex(at: 0.08) == 1)
        // 第二轮开头回到第 0 帧，播完停在最后一帧
        precondition(asset.frameIndex(at: 0.26) == 0 && asset.frameIndex(at: 0.6) == 1)
        // 距离下一帧的时间
        precondition(abs(asset.timeUntilNextFrame(at: 0.08)! - 0.17) < 0.001)
        // 播完后不再有下一帧（无需继续唤醒）
        precondition(asset.timeUntilNextFrame(at: 0.5) == nil)
        // 两帧像素确实不同
        precondition(asset.frames[0].dataProvider?.data as Data? != asset.frames[1].dataProvider?.data as Data?)
        // 播放时钟：暂停期间不累计时间
        let start = ContinuousClock.now
        var clock = WatchImagePlaybackClock()
        clock.update(assetID: asset.id, playing: true, at: start)
        // 播放 0.08 秒后暂停
        clock.setPlaying(false, at: start.advanced(by: .seconds(0.08)))
        // 一小时后恢复，已播放时长仍是 0.08 秒
        let resume = start.advanced(by: .seconds(3600))
        precondition(abs(clock.elapsed(at: resume) - 0.08) < 0.001)
        // 恢复后继续累计
        clock.update(assetID: asset.id, playing: true, at: resume)
        precondition(abs(clock.elapsed(at: resume.advanced(by: .seconds(0.2))) - 0.28) < 0.001)
        // 换新图片则从零开始
        clock.update(assetID: "new", playing: false, at: resume)
        precondition(clock.elapsed(at: resume.advanced(by: .seconds(1))) == 0)
        // 符号链接必须被拒绝
        let link = result.appendingPathComponent("symlink")
        try? FileManager.default.removeItem(at: link)
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: gif)
        do { _ = try WatchImageAsset.decode(contentsOf: link); preconditionFailure("symlink accepted") } catch { }
        // 非图片内容必须被拒绝
        let invalid = result.appendingPathComponent("invalid")
        try Data("bad image".utf8).write(to: invalid)
        do { _ = try WatchImageAsset.decode(contentsOf: invalid); preconditionFailure("invalid image accepted") } catch { }
        // 写出通过的检查项报告
        let report: [String: Any] = ["result": "passed", "checks": ["staticPng", "256pxBound", "2MiBDecodedBound", "gifDistinctFrames", "originalFrameTiming", "finiteGifTwoPlays", "finiteStopsWithoutWakeups", "pauseDoesNotConsumeTime", "resumeContinues", "newAssetRestarts", "symlinkRejected", "invalidImageRejected"]]
        try FileManager.default.createDirectory(at: reportURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys]).write(to: reportURL)
        print("12 image checks passed")
    }
}
