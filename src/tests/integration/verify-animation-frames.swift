// 引入 Foundation、CoreGraphics 与 ImageIO
import Foundation
import CoreGraphics
import ImageIO

// 检查实际的原生截图像素。夹具是纯红/蓝;
// 普通文本和主题色达不到这些饱和度阈值。
// 每张截图的统计结果
var frames: [[String: Any]] = []
// 出现过的主色
var seen = Set<String>()
// 逐张检查原生截图的真实像素；夹具是纯红纯蓝，普通文字和主题色满足不了这么饱和的阈值
for argument in CommandLine.arguments.dropFirst() {
    let url = URL(fileURLWithPath: argument)
    // 解码截图
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
        fatalError("Cannot decode screenshot: \(argument)")
    }
    let size = image.width * image.height * 4
    // 分配 RGBA 缓冲区并把截图画进去
    let pixels = UnsafeMutablePointer<UInt8>.allocate(capacity: size)
    defer { pixels.deallocate() }
    guard let context = CGContext(data: pixels, width: image.width, height: image.height,
                                  bitsPerComponent: 8, bytesPerRow: image.width * 4,
                                  space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue |
                                    CGBitmapInfo.byteOrder32Big.rawValue) else {
        fatalError("Cannot create screenshot pixel context")
    }
    // 把图片绘制进位图上下文
    context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
    // 统计饱和红色和蓝色像素
    var red = 0
    var blue = 0
    for offset in stride(from: 0, to: size, by: 4) {
        // 红：R 高，G、B 低
        if pixels[offset] > 200 && pixels[offset + 1] < 80 && pixels[offset + 2] < 80 { red += 1 }
        // 蓝：B 高，R、G 低
        if pixels[offset + 2] > 200 && pixels[offset] < 80 && pixels[offset + 1] < 80 { blue += 1 }
    }
    // 超过 1000 个像素才算该颜色为主色
    let dominant = max(red, blue) > 1000 ? (red > blue ? "red" : "blue") : "neither"
    seen.insert(dominant)
    frames.append(["file": url.lastPathComponent, "redPixels": red, "bluePixels": blue, "dominant": dominant])
}
// 红蓝两种颜色都出现过，才说明渲染出的帧确实在变化
let passed = seen.contains("red") && seen.contains("blue")
let report: [String: Any] = ["changingRenderedFrames": passed, "frames": frames]
let json = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
print(String(decoding: json, as: UTF8.self))
if !passed { exit(1) }
