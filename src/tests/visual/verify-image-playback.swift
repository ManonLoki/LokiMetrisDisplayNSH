// 引入 Foundation、CoreGraphics 与 ImageIO
import Foundation
import CoreGraphics
import ImageIO

// 只用截图作判据:不导入解码器、播放器、已过帧数的计算或 JS 状态。
// 只看截图的检验：不引入解码器、播放器、帧时间计算或 JS 状态
struct Dimensions: Decodable { let width: Double; let height: Double }
struct Rect: Decodable { let x: Double; let y: Double; let width: Double; let height: Double }
// 两张图片的矩形
struct Rects: Decodable { let a: Rect; let b: Rect }
// 门控与加载状态
struct Gates: Decodable {
    let source: String
    let activeA: Bool; let activeB: Bool; let foreground: Bool; let reducedMotion: Bool
    let loadedA: Bool; let loadedB: Bool; let error: String
}
// 一次截图的元数据
struct Capture: Decodable {
    let file: String; let group: String; let index: Int
    let viewport: Dimensions; let rects: Rects; let gates: Gates
    let timeMs: Double; let elapsedSinceReadyMs: Double
}
struct Manifest: Decodable { let captures: [Capture] }
// 一块区域的采样结果：主色、占比与像素哈希
struct Sample { let color: String; let ratio: Double; let hash: String }
// 解码后的像素图
struct Pixels {
    let width: Int; let height: Int; let bytes: [UInt8]

    // 以 URL 初始化
    init(_ url: URL) throws {
        // 创建图片源
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              // 解码首帧
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            // 失败则抛错
            throw NSError(domain: "GifPixels", code: 1, userInfo: [NSLocalizedDescriptionKey: "Cannot decode \(url.path)"])
        }
        // 记录宽高
        width = image.width; height = image.height
        // 分配像素缓冲
        var storage = [UInt8](repeating: 0, count: width * height * 4)
        // 绘制进缓冲
        let rendered = storage.withUnsafeMutableBytes { buffer -> Bool in
            // 创建位图上下文
            guard let context = CGContext(data: buffer.baseAddress, width: image.width, height: image.height,
                // 每分量 8 位，每行字节数，RGB 色彩空间
                bitsPerComponent: 8, bytesPerRow: image.width * 4, space: CGColorSpaceCreateDeviceRGB(),
                // 预乘 Alpha 与大端字节序
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue) else { return false }
            // 把图片绘制进上下文
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
            // 绘制成功
            return true
        }
        // 绘制失败则抛错
        guard rendered else { throw NSError(domain: "GifPixels", code: 2) }
        // 保存像素
        bytes = storage
    }

    // 取视口坐标处的颜色，可翻转行顺序
    func rgb(_ x: Double, _ y: Double, _ viewport: Dimensions, _ flipped: Bool) -> [UInt8] {
        let px = min(width - 1, max(0, Int(x * Double(width) / viewport.width)))
        let rawY = min(height - 1, max(0, Int(y * Double(height) / viewport.height)))
        let py = flipped ? height - rawY - 1 : rawY
        let offset = (py * width + px) * 4
        return Array(bytes[offset..<offset + 3])
    }

    // 统计左上角固定绿色标记里的绿色像素，用来确定位图的行方向
    func anchor(_ viewport: Dimensions, _ flipped: Bool) -> Int {
        var green = 0
        for y in 7...12 { for x in 7...12 {
            let value = rgb(Double(x), Double(y), viewport, flipped)
            if value[1] > 180 && value[0] < 80 && value[2] < 80 { green += 1 }
        } }
        return green
    }

    // 采样图片区域：统计红、蓝像素占比，并对所有像素算哈希，用来判断画面是否变化
    func sample(_ rect: Rect, _ viewport: Dimensions, _ flipped: Bool) -> Sample {
        var red = 0, blue = 0, count = 0
        var hash: UInt64 = 14695981039346656037
        // 保持在图片面内;排除其边缘处的控件和插值。
        // 只取图片内部，避开边缘的控件和插值
        for y in stride(from: rect.y + 20, through: rect.y + rect.height - 20, by: 2) {
            for x in stride(from: rect.x + 20, through: rect.x + rect.width - 20, by: 2) {
                let value = rgb(x, y, viewport, flipped)
                if value[0] > 160 && value[1] < 120 && value[2] < 100 { red += 1 }
                if value[2] > 160 && value[0] < 100 && value[1] < 150 { blue += 1 }
                for component in value { hash = (hash ^ UInt64(component)) &* 1099511628211 }
                count += 1
            }
        }
        // 主色与占比
        return Sample(color: red >= blue ? "red" : "blue", ratio: Double(max(red, blue)) / Double(max(1, count)),
                      // 哈希
                      hash: String(hash, radix: 16))
    }
}

// 参数：截图清单
guard CommandLine.arguments.count == 2 else { fatalError("usage: swift verify-image-playback.swift <captures.json>") }
let manifestURL = URL(fileURLWithPath: CommandLine.arguments[1])
let manifest = try JSONDecoder().decode(Manifest.self, from: Data(contentsOf: manifestURL))
let directory = manifestURL.deletingLastPathComponent()
var failures: [String] = []
func require(_ value: Bool, _ message: String) { if !value { failures.append(message) } }
// 每组应有的截图数
let counts = ["moving": 8, "inactive": 5, "resumed": 8, "independent": 8, "background": 5,
              "foreground-resumed": 8, "reduced": 5, "finite-running": 8, "finite-ended": 5]
// 共 60 张
require(manifest.captures.count == counts.values.reduce(0, +), "Expected 60 native screenshot captures")
guard let first = manifest.captures.first else { fatalError("No native screenshots") }
let reference = try Pixels(directory.appendingPathComponent(first.file))
// 用绿色标记确定行方向
let flipped = reference.anchor(first.viewport, true) > reference.anchor(first.viewport, false)
require(reference.anchor(first.viewport, flipped) >= 30, "Missing fixed green orientation marker")
var samples: [String: [Sample]] = [:]
var frameReports: [[String: Any]] = []
// 逐张检查
for capture in manifest.captures {
    let image = try Pixels(directory.appendingPathComponent(capture.file))
    // 宽高比必须与视口一致
    let scaleX = Double(image.width) / capture.viewport.width, scaleY = Double(image.height) / capture.viewport.height
    require(abs(scaleX - scaleY) < max(scaleX, scaleY) * 0.02, "Native viewport aspect mismatch: \(capture.file)")
    // 两张图都必须已加载且无错误
    require(capture.gates.loadedA && capture.gates.loadedB && capture.gates.error.isEmpty, "Images did not load: \(capture.file)")
    let pair = [image.sample(capture.rects.a, capture.viewport, flipped), image.sample(capture.rects.b, capture.viewport, flipped)]
    // 每个区域必须是纯色的红或蓝帧
    for sample in pair { require(sample.ratio > 0.97, "ROI is not a solid red/blue GIF frame: \(capture.file)") }
    samples[capture.file] = pair
    frameReports.append(["file": capture.file, "group": capture.group, "pixelWidth": image.width, "pixelHeight": image.height,
                         "a": ["color": pair[0].color, "solidFraction": pair[0].ratio, "hash": pair[0].hash],
                         "b": ["color": pair[1].color, "solidFraction": pair[1].ratio, "hash": pair[1].hash]])
}

// 某个消费者在一组截图里出现过的颜色集合
func colors(_ captures: [Capture], _ consumer: Int) -> Set<String> {
    Set(captures.compactMap { samples[$0.file]?[consumer].color })
}
// 出现过的不同画面哈希集合
func hashes(_ captures: [Capture], _ consumer: Int) -> Set<String> {
    Set(captures.compactMap { samples[$0.file]?[consumer].hash })
}
var groups: [[String: Any]] = []
// 逐组检查
for name in counts.keys.sorted() {
    let captures = manifest.captures.filter { $0.group == name }.sorted { $0.index < $1.index }
    require(captures.count == counts[name], "Incomplete native capture group: \(name)")
    guard let first = captures.first, let last = captures.last else { continue }
    let gate = first.gates
    // 每组应有的门控组合
    let expected: Bool
    switch name {
    // A 活跃、B 不活跃、前台、未减弱动态
    case "moving", "resumed", "finite-running", "finite-ended":
        expected = gate.activeA && !gate.activeB && gate.foreground && !gate.reducedMotion
    // 全部不活跃
    case "inactive": expected = !gate.activeA && !gate.activeB && gate.foreground && !gate.reducedMotion
    // A 停止、B 活跃：两个消费者互不影响
    case "independent", "foreground-resumed": expected = !gate.activeA && gate.activeB && gate.foreground && !gate.reducedMotion
    // 后台：全部停止
    case "background": expected = !gate.activeA && gate.activeB && !gate.foreground && !gate.reducedMotion
    // 减弱动态效果
    default: expected = gate.activeA && !gate.activeB && gate.foreground && gate.reducedMotion
    }
    require(expected, "Wrong requested motion gates: \(name)")
    // 哪个消费者应在动
    let movingA = ["moving", "resumed", "finite-running"].contains(name)
    let movingB = ["independent", "foreground-resumed"].contains(name)
    // 在动的必须同时出现红和蓝；静止的画面哈希只能有一个
    for consumer in 0...1 {
        let moving = consumer == 0 ? movingA : movingB
        if moving {
            require(colors(captures, consumer) == Set(["red", "blue"]), "Consumer \(consumer) never displayed both native GIF colors: \(name)")
        } else {
            require(hashes(captures, consumer).count == 1, "Paused consumer \(consumer) changed pixels: \(name)")
        }
    }
    // 采样跨度要足以判断动或停
    require(last.timeMs - first.timeMs >= 2200, "Capture interval too short to establish motion/pause: \(name)")
    // 有限循环的 GIF 播完后必须停在最后的蓝帧，且已过了应有的重复时间
    if name == "finite-ended" {
        require(colors(captures, 0) == Set(["blue"]), "Finite GIF did not stop on its final blue frame")
        require(first.elapsedSinceReadyMs >= 5000, "Finite animation was checked before its permitted repeats elapsed")
    }
    // 减弱动态效果时不得前进到第一帧之外
    if name == "reduced" { require(colors(captures, 0) == Set(["red"]), "Reduced motion advanced beyond the initial GIF frame") }
    groups.append(["group": name, "aColors": colors(captures, 0).sorted(), "bColors": colors(captures, 1).sorted(),
                   "aUniqueHashes": hashes(captures, 0).count, "bUniqueHashes": hashes(captures, 1).count,
                   "spanMs": last.timeMs - first.timeMs])
}
// 汇总报告，并说明范围：真实的 macOS 截图像素，不推断真机生命周期或剩余帧时间
let report: [String: Any] = ["passed": failures.isEmpty, "failures": failures, "bitmapRowsFlipped": flipped,
    "scope": "Real macOS screenshot pixels: red/blue GIF playback, active and explicit scope-foreground freeze/resume, reduced motion, independent consumers of one URI, finite-loop final frame. No device lifecycle or remaining-frame timing is inferred from screenshots.",
    "groups": groups, "frames": frameReports]
let result = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
print(String(decoding: result, as: UTF8.self))
if !failures.isEmpty { exit(1) }
