import Foundation
import CoreGraphics
import ImageIO

// Read native screenshots only. No animation implementation or phase is imported.
struct Dimensions: Decodable { let width: Double; let height: Double }
struct Frame: Decodable { let x: Double; let y: Double; let width: Double; let height: Double }
struct Gates: Decodable { let active: Bool; let foreground: Bool; let reducedMotion: Bool }
struct Capture: Decodable {
    let file: String
    let group: String
    let index: Int
    let viewport: Dimensions
    let rect: Frame
    let gates: Gates
    let timeMs: Double
}
struct Visual: Decodable { let file: String; let width: Double; let height: Double; let page: String }
struct Manifest: Decodable { let captures: [Capture]; let visuals: [Visual] }
struct Image {
    let width: Int
    let height: Int
    let pixels: [UInt8]

    init(_ url: URL) throws {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            throw NSError(domain: "CardPixels", code: 1, userInfo: [NSLocalizedDescriptionKey: "Cannot decode \(url.path)"])
        }
        width = image.width; height = image.height
        var buffer = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = buffer.withUnsafeMutableBytes { bytes -> Bool in
            guard let context = CGContext(data: bytes.baseAddress, width: image.width, height: image.height,
                bitsPerComponent: 8, bytesPerRow: image.width * 4, space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue) else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
            return true
        }
        guard drawn else { throw NSError(domain: "CardPixels", code: 2) }
        pixels = buffer
    }

    func rgb(_ x: Double, _ y: Double, viewport: Dimensions, flipped: Bool) -> [Double] {
        let px = max(0, min(width - 1, Int(x * Double(width) / viewport.width)))
        let rawY = max(0, min(height - 1, Int(y * Double(height) / viewport.height)))
        let py = flipped ? height - rawY - 1 : rawY
        let offset = (py * width + px) * 4
        return (0..<3).map { Double(pixels[offset + $0]) }
    }
}

let sides = ["top", "right", "bottom", "left"]
let samplesPerSide = 64

func energy(_ rgb: [Double]) -> Double { max(rgb[1], rgb[2]) - rgb[0] * 0.15 }

func edgeSamples(_ image: Image, _ capture: Capture, flipped: Bool, interior: Bool = false) -> [Double] {
    let rect = capture.rect
    var result: [Double] = []
    for side in 0..<4 {
        for index in 0..<samplesPerSide {
            let along = (Double(index) + 0.5) / Double(samplesPerSide)
            var maximum = 0.0
            // The rim is 1.6pt; sample a small inward strip to include antialiasing and transparent layout borders.
            for inset in stride(from: interior ? 8.0 : 0.25, through: interior ? 8.0 : 3.75, by: 0.5) {
                let x: Double, y: Double
                switch side {
                case 0: x = rect.x + 20 + along * (rect.width - 40); y = rect.y + inset
                case 1: x = rect.x + rect.width - inset; y = rect.y + 20 + along * (rect.height - 40)
                case 2: x = rect.x + 20 + along * (rect.width - 40); y = rect.y + rect.height - inset
                default: x = rect.x + inset; y = rect.y + 20 + along * (rect.height - 40)
                }
                maximum = max(maximum, energy(image.rgb(x, y, viewport: capture.viewport, flipped: flipped)))
            }
            result.append(maximum)
        }
    }
    return result
}

func means(_ values: [Double]) -> [Double] {
    (0..<4).map { side in
        values[(side * samplesPerSide)..<((side + 1) * samplesPerSide)].reduce(0, +) / Double(samplesPerSide)
    }
}

func cornerContrasts(_ image: Image, _ capture: Capture, flipped: Bool) -> [Double] {
    let rect = capture.rect
    return [(false, false), (true, false), (true, true), (false, true)].map { right, bottom in
        let outerX = rect.x + (right ? rect.width - 0.25 : 0.25)
        let outerY = rect.y + (bottom ? rect.height - 0.25 : 0.25)
        let arcX = rect.x + (right ? rect.width - 5.8 : 5.8)
        let arcY = rect.y + (bottom ? rect.height - 5.8 : 5.8)
        let outside = energy(image.rgb(outerX, outerY, viewport: capture.viewport, flipped: flipped))
        var arc = 0.0
        for dx in stride(from: -2.0, through: 2.0, by: 0.5) {
            for dy in stride(from: -2.0, through: 2.0, by: 0.5) {
                arc = max(arc, energy(image.rgb(arcX + dx, arcY + dy, viewport: capture.viewport, flipped: flipped)))
            }
        }
        return arc - outside
    }
}

guard CommandLine.arguments.count == 2 else { fatalError("usage: swift verify-card-frames.swift <captures.json>") }
let manifestURL = URL(fileURLWithPath: CommandLine.arguments[1])
let manifest = try JSONDecoder().decode(Manifest.self, from: Data(contentsOf: manifestURL))
let directory = manifestURL.deletingLastPathComponent()
var failures: [String] = []
func require(_ passed: Bool, _ message: String) { if !passed { failures.append(message) } }
let orbit = manifest.captures.filter { $0.group == "orbit" }.sorted { $0.index < $1.index }
require(orbit.count == 12, "Expected 12 native orbit screenshots")
guard let reference = orbit.first else { fatalError("No orbit captures") }
let first = try Image(directory.appendingPathComponent(reference.file))
// Bitmap contexts differ in row convention; choose the orientation with the actual card rim at the declared bounds.
let flipped = edgeSamples(first, reference, flipped: true).reduce(0, +)
    > edgeSamples(first, reference, flipped: false).reduce(0, +)
var samples: [String: [Double]] = [:]
var cornerMaximum = [Double](repeating: 0, count: 4)
var frameReports: [[String: Any]] = []
for capture in manifest.captures {
    let image = try Image(directory.appendingPathComponent(capture.file))
    let scaleX = Double(image.width) / capture.viewport.width
    let scaleY = Double(image.height) / capture.viewport.height
    require(abs(scaleX - scaleY) < max(scaleX, scaleY) * 0.02, "Screenshot aspect does not match viewport: \(capture.file)")
    let values = edgeSamples(image, capture, flipped: flipped)
    samples[capture.file] = values
    let sideMeans = means(values)
    require(sideMeans.min()! > 35, "A card side lost its visible cyan/violet rim: \(capture.file)")
    let interiorMeans = means(edgeSamples(image, capture, flipped: flipped, interior: true))
    let rimContrast = zip(sideMeans, interiorMeans).map { $0 - $1 }
    require(rimContrast.allSatisfy { $0 > 7 }, "Card rim is not visible above the adjacent inner face: \(capture.file)")
    let contrasts = cornerContrasts(image, capture, flipped: flipped)
    for index in 0..<4 { cornerMaximum[index] = max(cornerMaximum[index], contrasts[index]) }
    frameReports.append(["file": capture.file, "group": capture.group,
                         "pixelWidth": image.width, "pixelHeight": image.height,
                         "edgeEnergy": Dictionary(uniqueKeysWithValues: zip(sides, sideMeans)),
                         "rimContrast": rimContrast,
                         "roundedCornerContrast": contrasts])
}
require(cornerMaximum.allSatisfy { $0 > 10 }, "Rounded corner arcs were not visible against their outside corners")

let orbitSamples = orbit.compactMap { samples[$0.file] }
let baseline = (0..<(samplesPerSide * 4)).map { index in orbitSamples.map { $0[index] }.min() ?? 0 }
var dominantSides = Set<String>()
var peakRange = [Double](repeating: 0, count: 4)
var orbitReports: [[String: Any]] = []
for (capture, values) in zip(orbit, orbitSamples) {
    let moving = means(zip(values, baseline).map { max(0, $0 - $1) })
    let dominant = moving.indices.max { moving[$0] < moving[$1] }!
    if moving[dominant] > 8 { dominantSides.insert(sides[dominant]) }
    for index in 0..<4 { peakRange[index] = max(peakRange[index], moving[index]) }
    orbitReports.append(["file": capture.file, "dominantSide": sides[dominant],
                         "movingEnergy": Dictionary(uniqueKeysWithValues: zip(sides, moving))])
}
require(dominantSides.count == 4, "Bright edge did not travel to all four sides: \(dominantSides.sorted())")
require(peakRange.allSatisfy { $0 > 8 }, "One or more border sides did not change brightness")
if let last = orbit.last { require(last.timeMs - reference.timeMs >= 8000, "Orbit sampling did not cover an eight-second cycle") }

var pausedReports: [[String: Any]] = []
for group in ["inactive", "background", "reduced", "resumed"] {
    let captures = manifest.captures.filter { $0.group == group }.sorted { $0.index < $1.index }
    require(captures.count == 2, "Expected a native screenshot pair for \(group)")
    guard captures.count == 2, let a = samples[captures[0].file], let b = samples[captures[1].file] else { continue }
    let changes = zip(a, b).map { abs($0 - $1) }
    let average = changes.reduce(0, +) / Double(changes.count)
    let fraction = Double(changes.filter { $0 > 6 }.count) / Double(changes.count)
    let gate = captures[0].gates
    let expected = group == "inactive" ? !gate.active : group == "background" ? !gate.foreground
        : group == "reduced" ? gate.reducedMotion : gate.active && gate.foreground && !gate.reducedMotion
    require(expected, "The requested motion gate was not set: \(group)")
    if group == "resumed" {
        require(average > 4 && fraction > 0.03, "Ring did not resume changing after active became true")
    } else {
        require(average <= 2 && fraction <= 0.02, "Ring kept moving with \(group) gate")
    }
    pausedReports.append(["group": group, "averageEdgeDifference": average, "changedSampleFraction": fraction])
}
var visualReports: [[String: Any]] = []
for visual in manifest.visuals {
    let image = try Image(directory.appendingPathComponent(visual.file))
    let scaleX = Double(image.width) / visual.width, scaleY = Double(image.height) / visual.height
    require(abs(scaleX - scaleY) < max(scaleX, scaleY) * 0.02, "Layout screenshot aspect mismatch: \(visual.file)")
    var lit = 0
    for offset in stride(from: 0, to: image.pixels.count, by: 4) {
        if max(image.pixels[offset], image.pixels[offset + 1], image.pixels[offset + 2]) > 80 { lit += 1 }
    }
    require(lit > 1000, "Layout screenshot contains no visible card content: \(visual.file)")
    visualReports.append(["file": visual.file, "pixelWidth": image.width, "pixelHeight": image.height, "litPixels": lit])
}
require(manifest.visuals.count == 4, "Expected phone and desktop panel/settings screenshots")
let report: [String: Any] = ["passed": failures.isEmpty, "failures": failures,
    "scope": "Real macOS screenshot pixels: four-side edge motion, rounded rim, explicit active/foreground gates, reduced motion and resumed motion. Phone dimensions emulate a viewport; no physical-device lifecycle or aesthetic judgement is claimed.",
    "bitmapRowsFlipped": flipped, "visitedSides": dominantSides.sorted(), "cornerContrast": cornerMaximum,
    "orbit": orbitReports, "pausedPairs": pausedReports, "frames": frameReports, "layouts": visualReports]
let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
print(String(decoding: data, as: UTF8.self))
if !failures.isEmpty { exit(1) }
