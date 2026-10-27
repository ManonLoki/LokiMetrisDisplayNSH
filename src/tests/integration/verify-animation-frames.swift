import Foundation
import CoreGraphics
import ImageIO

// Inspect actual native screenshot pixels. The fixture is solid red/blue;
// ordinary text and theme colors do not satisfy these saturated thresholds.
var frames: [[String: Any]] = []
var seen = Set<String>()
for argument in CommandLine.arguments.dropFirst() {
    let url = URL(fileURLWithPath: argument)
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
        fatalError("Cannot decode screenshot: \(argument)")
    }
    let size = image.width * image.height * 4
    let pixels = UnsafeMutablePointer<UInt8>.allocate(capacity: size)
    defer { pixels.deallocate() }
    guard let context = CGContext(data: pixels, width: image.width, height: image.height,
                                  bitsPerComponent: 8, bytesPerRow: image.width * 4,
                                  space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue |
                                    CGBitmapInfo.byteOrder32Big.rawValue) else {
        fatalError("Cannot create screenshot pixel context")
    }
    context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
    var red = 0
    var blue = 0
    for offset in stride(from: 0, to: size, by: 4) {
        if pixels[offset] > 200 && pixels[offset + 1] < 80 && pixels[offset + 2] < 80 { red += 1 }
        if pixels[offset + 2] > 200 && pixels[offset] < 80 && pixels[offset + 1] < 80 { blue += 1 }
    }
    let dominant = max(red, blue) > 1000 ? (red > blue ? "red" : "blue") : "neither"
    seen.insert(dominant)
    frames.append(["file": url.lastPathComponent, "redPixels": red, "bluePixels": blue, "dominant": dominant])
}
let passed = seen.contains("red") && seen.contains("blue")
let report: [String: Any] = ["changingRenderedFrames": passed, "frames": frames]
let json = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
print(String(decoding: json, as: UTF8.self))
if !passed { exit(1) }
