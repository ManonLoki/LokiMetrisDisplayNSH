import Foundation
import CoreGraphics
import ImageIO

// Screenshot-only oracle: no decoder, player, elapsed-frame calculation, or JS state is imported.
struct Dimensions: Decodable { let width: Double; let height: Double }
struct Rect: Decodable { let x: Double; let y: Double; let width: Double; let height: Double }
struct Rects: Decodable { let a: Rect; let b: Rect }
struct Gates: Decodable {
    let source: String
    let activeA: Bool; let activeB: Bool; let foreground: Bool; let reducedMotion: Bool
    let loadedA: Bool; let loadedB: Bool; let error: String
}
struct Capture: Decodable {
    let file: String; let group: String; let index: Int
    let viewport: Dimensions; let rects: Rects; let gates: Gates
    let timeMs: Double; let elapsedSinceReadyMs: Double
}
struct Manifest: Decodable { let captures: [Capture] }
struct Sample { let color: String; let ratio: Double; let hash: String }
struct Pixels {
    let width: Int; let height: Int; let bytes: [UInt8]

    init(_ url: URL) throws {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            throw NSError(domain: "GifPixels", code: 1, userInfo: [NSLocalizedDescriptionKey: "Cannot decode \(url.path)"])
        }
        width = image.width; height = image.height
        var storage = [UInt8](repeating: 0, count: width * height * 4)
        let rendered = storage.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(data: buffer.baseAddress, width: image.width, height: image.height,
                bitsPerComponent: 8, bytesPerRow: image.width * 4, space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue) else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
            return true
        }
        guard rendered else { throw NSError(domain: "GifPixels", code: 2) }
        bytes = storage
    }

    func rgb(_ x: Double, _ y: Double, _ viewport: Dimensions, _ flipped: Bool) -> [UInt8] {
        let px = min(width - 1, max(0, Int(x * Double(width) / viewport.width)))
        let rawY = min(height - 1, max(0, Int(y * Double(height) / viewport.height)))
        let py = flipped ? height - rawY - 1 : rawY
        let offset = (py * width + px) * 4
        return Array(bytes[offset..<offset + 3])
    }

    func anchor(_ viewport: Dimensions, _ flipped: Bool) -> Int {
        var green = 0
        for y in 7...12 { for x in 7...12 {
            let value = rgb(Double(x), Double(y), viewport, flipped)
            if value[1] > 180 && value[0] < 80 && value[2] < 80 { green += 1 }
        } }
        return green
    }

    func sample(_ rect: Rect, _ viewport: Dimensions, _ flipped: Bool) -> Sample {
        var red = 0, blue = 0, count = 0
        var hash: UInt64 = 14695981039346656037
        // Stay inside the image face; controls and interpolation at its edge are excluded.
        for y in stride(from: rect.y + 20, through: rect.y + rect.height - 20, by: 2) {
            for x in stride(from: rect.x + 20, through: rect.x + rect.width - 20, by: 2) {
                let value = rgb(x, y, viewport, flipped)
                if value[0] > 160 && value[1] < 120 && value[2] < 100 { red += 1 }
                if value[2] > 160 && value[0] < 100 && value[1] < 150 { blue += 1 }
                for component in value { hash = (hash ^ UInt64(component)) &* 1099511628211 }
                count += 1
            }
        }
        return Sample(color: red >= blue ? "red" : "blue", ratio: Double(max(red, blue)) / Double(max(1, count)),
                      hash: String(hash, radix: 16))
    }
}

guard CommandLine.arguments.count == 2 else { fatalError("usage: swift verify-image-playback.swift <captures.json>") }
let manifestURL = URL(fileURLWithPath: CommandLine.arguments[1])
let manifest = try JSONDecoder().decode(Manifest.self, from: Data(contentsOf: manifestURL))
let directory = manifestURL.deletingLastPathComponent()
var failures: [String] = []
func require(_ value: Bool, _ message: String) { if !value { failures.append(message) } }
let counts = ["moving": 8, "inactive": 5, "resumed": 8, "independent": 8, "background": 5,
              "foreground-resumed": 8, "reduced": 5, "finite-running": 8, "finite-ended": 5]
require(manifest.captures.count == counts.values.reduce(0, +), "Expected 60 native screenshot captures")
guard let first = manifest.captures.first else { fatalError("No native screenshots") }
let reference = try Pixels(directory.appendingPathComponent(first.file))
let flipped = reference.anchor(first.viewport, true) > reference.anchor(first.viewport, false)
require(reference.anchor(first.viewport, flipped) >= 30, "Missing fixed green orientation marker")
var samples: [String: [Sample]] = [:]
var frameReports: [[String: Any]] = []
for capture in manifest.captures {
    let image = try Pixels(directory.appendingPathComponent(capture.file))
    let scaleX = Double(image.width) / capture.viewport.width, scaleY = Double(image.height) / capture.viewport.height
    require(abs(scaleX - scaleY) < max(scaleX, scaleY) * 0.02, "Native viewport aspect mismatch: \(capture.file)")
    require(capture.gates.loadedA && capture.gates.loadedB && capture.gates.error.isEmpty, "Images did not load: \(capture.file)")
    let pair = [image.sample(capture.rects.a, capture.viewport, flipped), image.sample(capture.rects.b, capture.viewport, flipped)]
    for sample in pair { require(sample.ratio > 0.97, "ROI is not a solid red/blue GIF frame: \(capture.file)") }
    samples[capture.file] = pair
    frameReports.append(["file": capture.file, "group": capture.group, "pixelWidth": image.width, "pixelHeight": image.height,
                         "a": ["color": pair[0].color, "solidFraction": pair[0].ratio, "hash": pair[0].hash],
                         "b": ["color": pair[1].color, "solidFraction": pair[1].ratio, "hash": pair[1].hash]])
}

func colors(_ captures: [Capture], _ consumer: Int) -> Set<String> {
    Set(captures.compactMap { samples[$0.file]?[consumer].color })
}
func hashes(_ captures: [Capture], _ consumer: Int) -> Set<String> {
    Set(captures.compactMap { samples[$0.file]?[consumer].hash })
}
var groups: [[String: Any]] = []
for name in counts.keys.sorted() {
    let captures = manifest.captures.filter { $0.group == name }.sorted { $0.index < $1.index }
    require(captures.count == counts[name], "Incomplete native capture group: \(name)")
    guard let first = captures.first, let last = captures.last else { continue }
    let gate = first.gates
    let expected: Bool
    switch name {
    case "moving", "resumed", "finite-running", "finite-ended":
        expected = gate.activeA && !gate.activeB && gate.foreground && !gate.reducedMotion
    case "inactive": expected = !gate.activeA && !gate.activeB && gate.foreground && !gate.reducedMotion
    case "independent", "foreground-resumed": expected = !gate.activeA && gate.activeB && gate.foreground && !gate.reducedMotion
    case "background": expected = !gate.activeA && gate.activeB && !gate.foreground && !gate.reducedMotion
    default: expected = gate.activeA && !gate.activeB && gate.foreground && gate.reducedMotion
    }
    require(expected, "Wrong requested motion gates: \(name)")
    let movingA = ["moving", "resumed", "finite-running"].contains(name)
    let movingB = ["independent", "foreground-resumed"].contains(name)
    for consumer in 0...1 {
        let moving = consumer == 0 ? movingA : movingB
        if moving {
            require(colors(captures, consumer) == Set(["red", "blue"]), "Consumer \(consumer) never displayed both native GIF colors: \(name)")
        } else {
            require(hashes(captures, consumer).count == 1, "Paused consumer \(consumer) changed pixels: \(name)")
        }
    }
    require(last.timeMs - first.timeMs >= 2200, "Capture interval too short to establish motion/pause: \(name)")
    if name == "finite-ended" {
        require(colors(captures, 0) == Set(["blue"]), "Finite GIF did not stop on its final blue frame")
        require(first.elapsedSinceReadyMs >= 5000, "Finite animation was checked before its permitted repeats elapsed")
    }
    if name == "reduced" { require(colors(captures, 0) == Set(["red"]), "Reduced motion advanced beyond the initial GIF frame") }
    groups.append(["group": name, "aColors": colors(captures, 0).sorted(), "bColors": colors(captures, 1).sorted(),
                   "aUniqueHashes": hashes(captures, 0).count, "bUniqueHashes": hashes(captures, 1).count,
                   "spanMs": last.timeMs - first.timeMs])
}
let report: [String: Any] = ["passed": failures.isEmpty, "failures": failures, "bitmapRowsFlipped": flipped,
    "scope": "Real macOS screenshot pixels: red/blue GIF playback, active and explicit scope-foreground freeze/resume, reduced motion, independent consumers of one URI, finite-loop final frame. No device lifecycle or remaining-frame timing is inferred from screenshots.",
    "groups": groups, "frames": frameReports]
let result = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
print(String(decoding: result, as: UTF8.self))
if !failures.isEmpty { exit(1) }
