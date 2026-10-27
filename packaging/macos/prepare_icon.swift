// Packaging adaptation only: preserve the original artwork and let macOS apply
// its single native icon mask. Transparent padding otherwise receives a white
// legacy-icon plate in Finder on recent macOS versions.
import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

guard CommandLine.arguments.count == 3 else {
    fatalError("Usage: prepare_icon.swift <original.png> <opaque-macos.png>")
}
let input = URL(fileURLWithPath: CommandLine.arguments[1])
let output = URL(fileURLWithPath: CommandLine.arguments[2])
guard let source = CGImageSourceCreateWithURL(input as CFURL, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
      image.width == image.height, image.width <= 4096,
      let space = CGColorSpace(name: CGColorSpace.sRGB),
      let canvas = CGContext(data: nil, width: image.width, height: image.height,
                             bitsPerComponent: 8, bytesPerRow: image.width * 4,
                             space: space,
                             bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
else { fatalError("Cannot read the original Vortia icon") }
let bounds = CGRect(x: 0, y: 0, width: image.width, height: image.height)
// Existing original-logo background: sRGB #012050. No crop, rescale or redraw
// of the glyph is performed here; only its transparent canvas is filled.
canvas.setFillColor(red: 1.0 / 255.0, green: 32.0 / 255.0, blue: 80.0 / 255.0, alpha: 1)
canvas.fill(bounds)
canvas.draw(image, in: bounds)
guard let result = canvas.makeImage(),
      let destination = CGImageDestinationCreateWithURL(output as CFURL,
          UTType.png.identifier as CFString, 1, nil)
else { fatalError("Cannot write the native Vortia icon") }
CGImageDestinationAddImage(destination, result, nil)
guard CGImageDestinationFinalize(destination) else { fatalError("Cannot finish the native Vortia icon") }
