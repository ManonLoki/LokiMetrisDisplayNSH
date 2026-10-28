// 引入 Foundation、CoreGraphics 与 ImageIO
import Foundation
import CoreGraphics
import ImageIO

// 只读取原生截图。不导入任何动画实现或相位。
// 只读取原生截图，不引入任何动画实现或相位
struct Dimensions: Decodable { let width: Double; let height: Double }
// 卡片矩形
struct Frame: Decodable { let x: Double; let y: Double; let width: Double; let height: Double }
// 动效门控状态
struct Gates: Decodable { let active: Bool; let foreground: Bool; let reducedMotion: Bool }
// 一次截图的元数据
struct Capture: Decodable {
    let file: String
    let group: String
    let index: Int
    let viewport: Dimensions
    let rect: Frame
    let gates: Gates
    let timeMs: Double
}
// 布局截图的元数据
struct Visual: Decodable { let file: String; let width: Double; let height: Double; let page: String }
// 截图清单
struct Manifest: Decodable { let captures: [Capture]; let visuals: [Visual] }
// 解码后的像素图
struct Image {
    let width: Int
    let height: Int
    let pixels: [UInt8]

    // 解码并画进 RGBA 缓冲区
    init(_ url: URL) throws {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            throw NSError(domain: "CardPixels", code: 1, userInfo: [NSLocalizedDescriptionKey: "Cannot decode \(url.path)"])
        }
        width = image.width; height = image.height
        var buffer = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = buffer.withUnsafeMutableBytes { bytes -> Bool in
            // 创建位图上下文
            guard let context = CGContext(data: bytes.baseAddress, width: image.width, height: image.height,
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
        guard drawn else { throw NSError(domain: "CardPixels", code: 2) }
        // 保存像素
        pixels = buffer
    }

    // 取视口坐标处的颜色；可选择上下翻转，以适配不同位图的行顺序
    func rgb(_ x: Double, _ y: Double, viewport: Dimensions, flipped: Bool) -> [Double] {
        // 把视口坐标换算为像素坐标
        let px = max(0, min(width - 1, Int(x * Double(width) / viewport.width)))
        let rawY = max(0, min(height - 1, Int(y * Double(height) / viewport.height)))
        let py = flipped ? height - rawY - 1 : rawY
        let offset = (py * width + px) * 4
        return (0..<3).map { Double(pixels[offset + $0]) }
    }
}

// 四条边
let sides = ["top", "right", "bottom", "left"]
// 每条边采样 64 个点
let samplesPerSide = 64

// 颜色的“光带能量”：青紫色光带下 G、B 通道高于 R
func energy(_ rgb: [Double]) -> Double { max(rgb[1], rgb[2]) - rgb[0] * 0.15 }

// 沿四条边采样边缘光带的能量；光带宽 1.6pt，所以在向内一小段范围里取最大值，以涵盖抗锯齿和透明布局边
func edgeSamples(_ image: Image, _ capture: Capture, flipped: Bool, interior: Bool = false) -> [Double] {
    let rect = capture.rect
    var result: [Double] = []
    for side in 0..<4 {
        for index in 0..<samplesPerSide {
            // 沿边的位置（避开 20pt 的圆角区域）
            let along = (Double(index) + 0.5) / Double(samplesPerSide)
            var maximum = 0.0
            // 边缘宽 1.6pt;在内侧采样一小段条带,以包含抗锯齿和透明的布局边框。
            for inset in stride(from: interior ? 8.0 : 0.25, through: interior ? 8.0 : 3.75, by: 0.5) {
                let x: Double, y: Double
                // 上、右、下、左四边的坐标
                switch side {
                case 0: x = rect.x + 20 + along * (rect.width - 40); y = rect.y + inset
                case 1: x = rect.x + rect.width - inset; y = rect.y + 20 + along * (rect.height - 40)
                case 2: x = rect.x + 20 + along * (rect.width - 40); y = rect.y + rect.height - inset
                default: x = rect.x + inset; y = rect.y + 20 + along * (rect.height - 40)
                }
                maximum = max(maximum, energy(image.rgb(x, y, viewport: capture.viewport, flipped: flipped)))
            }
            // 追加最大值
            result.append(maximum)
        }
    }
    // 返回结果
    return result
}

// 每条边的平均能量
func means(_ values: [Double]) -> [Double] {
    (0..<4).map { side in
        values[(side * samplesPerSide)..<((side + 1) * samplesPerSide)].reduce(0, +) / Double(samplesPerSide)
    }
}

// 四个圆角处：圆弧上的能量减去圆角外的能量，用来确认圆角可见
func cornerContrasts(_ image: Image, _ capture: Capture, flipped: Bool) -> [Double] {
    let rect = capture.rect
    // 左上、右上、右下、左下
    return [(false, false), (true, false), (true, true), (false, true)].map { right, bottom in
        // 圆角外侧的点
        let outerX = rect.x + (right ? rect.width - 0.25 : 0.25)
        let outerY = rect.y + (bottom ? rect.height - 0.25 : 0.25)
        // 圆弧上的点
        let arcX = rect.x + (right ? rect.width - 5.8 : 5.8)
        let arcY = rect.y + (bottom ? rect.height - 5.8 : 5.8)
        let outside = energy(image.rgb(outerX, outerY, viewport: capture.viewport, flipped: flipped))
        // 在圆弧附近小范围取最大值
        var arc = 0.0
        for dx in stride(from: -2.0, through: 2.0, by: 0.5) {
            for dy in stride(from: -2.0, through: 2.0, by: 0.5) {
                arc = max(arc, energy(image.rgb(arcX + dx, arcY + dy, viewport: capture.viewport, flipped: flipped)))
            }
        }
        return arc - outside
    }
}

// 参数：截图清单
guard CommandLine.arguments.count == 2 else { fatalError("usage: swift verify-card-frames.swift <captures.json>") }
let manifestURL = URL(fileURLWithPath: CommandLine.arguments[1])
let manifest = try JSONDecoder().decode(Manifest.self, from: Data(contentsOf: manifestURL))
let directory = manifestURL.deletingLastPathComponent()
// 失败信息
var failures: [String] = []
// 记录不满足的条件
func require(_ passed: Bool, _ message: String) { if !passed { failures.append(message) } }
// 光带绕行的 12 张截图
let orbit = manifest.captures.filter { $0.group == "orbit" }.sorted { $0.index < $1.index }
require(orbit.count == 12, "Expected 12 native orbit screenshots")
guard let reference = orbit.first else { fatalError("No orbit captures") }
// 位图的行顺序可能不同；选择在声明的边界上确实出现卡片光边的那种方向
let first = try Image(directory.appendingPathComponent(reference.file))
// 位图上下文的行约定各不相同;选择实际卡片边缘落在声明边界处的方向。
let flipped = edgeSamples(first, reference, flipped: true).reduce(0, +)
    > edgeSamples(first, reference, flipped: false).reduce(0, +)
var samples: [String: [Double]] = [:]
var cornerMaximum = [Double](repeating: 0, count: 4)
var frameReports: [[String: Any]] = []
// 逐张检查
for capture in manifest.captures {
    let image = try Image(directory.appendingPathComponent(capture.file))
    // 截图宽高比必须与视口一致
    let scaleX = Double(image.width) / capture.viewport.width
    let scaleY = Double(image.height) / capture.viewport.height
    require(abs(scaleX - scaleY) < max(scaleX, scaleY) * 0.02, "Screenshot aspect does not match viewport: \(capture.file)")
    let values = edgeSamples(image, capture, flipped: flipped)
    samples[capture.file] = values
    // 每条边都必须有可见的青紫光边
    let sideMeans = means(values)
    require(sideMeans.min()! > 35, "A card side lost its visible cyan/violet rim: \(capture.file)")
    // 光边亮度必须明显高于紧邻的内侧面
    let interiorMeans = means(edgeSamples(image, capture, flipped: flipped, interior: true))
    let rimContrast = zip(sideMeans, interiorMeans).map { $0 - $1 }
    require(rimContrast.allSatisfy { $0 > 7 }, "Card rim is not visible above the adjacent inner face: \(capture.file)")
    let contrasts = cornerContrasts(image, capture, flipped: flipped)
    for index in 0..<4 { cornerMaximum[index] = max(cornerMaximum[index], contrasts[index]) }
    frameReports.append(["file": capture.file, "group": capture.group,
                         "pixelWidth": image.width, "pixelHeight": image.height,
                         "edgeEnergy": Dictionary(uniqueKeysWithValues: zip(sides, sideMeans)),
                         // 边缘对比度
                         "rimContrast": rimContrast,
                         // 圆角对比度
                         "roundedCornerContrast": contrasts])
}
// 圆角在外侧映衬下必须可见
require(cornerMaximum.allSatisfy { $0 > 10 }, "Rounded corner arcs were not visible against their outside corners")

// 光带绕行分析：每个采样点取各帧的最小值作为静态基线，超出基线的部分才是移动的光带
let orbitSamples = orbit.compactMap { samples[$0.file] }
let baseline = (0..<(samplesPerSide * 4)).map { index in orbitSamples.map { $0[index] }.min() ?? 0 }
var dominantSides = Set<String>()
var peakRange = [Double](repeating: 0, count: 4)
var orbitReports: [[String: Any]] = []
for (capture, values) in zip(orbit, orbitSamples) {
    // 每帧里光带最亮的一边
    let moving = means(zip(values, baseline).map { max(0, $0 - $1) })
    let dominant = moving.indices.max { moving[$0] < moving[$1] }!
    if moving[dominant] > 8 { dominantSides.insert(sides[dominant]) }
    for index in 0..<4 { peakRange[index] = max(peakRange[index], moving[index]) }
    orbitReports.append(["file": capture.file, "dominantSide": sides[dominant],
                         "movingEnergy": Dictionary(uniqueKeysWithValues: zip(sides, moving))])
}
// 亮边必须走遍四条边
require(dominantSides.count == 4, "Bright edge did not travel to all four sides: \(dominantSides.sorted())")
// 每条边的亮度都要有变化
require(peakRange.allSatisfy { $0 > 8 }, "One or more border sides did not change brightness")
// 采样时间要覆盖一个 8 秒周期
if let last = orbit.last { require(last.timeMs - reference.timeMs >= 8000, "Orbit sampling did not cover an eight-second cycle") }

// 静止分析：四组两两比较的截图
var pausedReports: [[String: Any]] = []
for group in ["inactive", "background", "reduced", "resumed"] {
    let captures = manifest.captures.filter { $0.group == group }.sorted { $0.index < $1.index }
    require(captures.count == 2, "Expected a native screenshot pair for \(group)")
    guard captures.count == 2, let a = samples[captures[0].file], let b = samples[captures[1].file] else { continue }
    // 对应采样点的亮度差
    let changes = zip(a, b).map { abs($0 - $1) }
    // 平均变化与变化显著的比例
    let average = changes.reduce(0, +) / Double(changes.count)
    let fraction = Double(changes.filter { $0 > 6 }.count) / Double(changes.count)
    // 门控状态必须与组名一致
    let gate = captures[0].gates
    let expected = group == "inactive" ? !gate.active : group == "background" ? !gate.foreground
        : group == "reduced" ? gate.reducedMotion : gate.active && gate.foreground && !gate.reducedMotion
    require(expected, "The requested motion gate was not set: \(group)")
    // 恢复后光带必须重新移动
    if group == "resumed" {
        require(average > 4 && fraction > 0.03, "Ring did not resume changing after active became true")
    // 其余三种情形光带必须静止
    } else {
        require(average <= 2 && fraction <= 0.02, "Ring kept moving with \(group) gate")
    }
    pausedReports.append(["group": group, "averageEdgeDifference": average, "changedSampleFraction": fraction])
}
// 布局截图检查：宽高比正确，且有足够多的亮像素（确有卡片内容）
var visualReports: [[String: Any]] = []
for visual in manifest.visuals {
    let image = try Image(directory.appendingPathComponent(visual.file))
    let scaleX = Double(image.width) / visual.width, scaleY = Double(image.height) / visual.height
    require(abs(scaleX - scaleY) < max(scaleX, scaleY) * 0.02, "Layout screenshot aspect mismatch: \(visual.file)")
    var lit = 0
    for offset in stride(from: 0, to: image.pixels.count, by: 4) {
        if max(image.pixels[offset], image.pixels[offset + 1], image.pixels[offset + 2]) > 80 { lit += 1 }
    }
    // 亮像素太少说明卡片没有内容
    require(lit > 1000, "Layout screenshot contains no visible card content: \(visual.file)")
    // 记录视觉报告
    visualReports.append(["file": visual.file, "pixelWidth": image.width, "pixelHeight": image.height, "litPixels": lit])
}
// 手机与桌面的展示台和设置页，共四张
require(manifest.visuals.count == 4, "Expected phone and desktop panel/settings screenshots")
// 汇总报告，并说明范围：真实的 macOS 截图像素，不声称真机生命周期或审美判断
let report: [String: Any] = ["passed": failures.isEmpty, "failures": failures,
    "scope": "Real macOS screenshot pixels: four-side edge motion, rounded rim, explicit active/foreground gates, reduced motion and resumed motion. Phone dimensions emulate a viewport; no physical-device lifecycle or aesthetic judgement is claimed.",
    "bitmapRowsFlipped": flipped, "visitedSides": dominantSides.sorted(), "cornerContrast": cornerMaximum,
    "orbit": orbitReports, "pausedPairs": pausedReports, "frames": frameReports, "layouts": visualReports]
let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
print(String(decoding: data, as: UTF8.self))
if !failures.isEmpty { exit(1) }
