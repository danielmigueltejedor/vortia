// Regenerate dmg-background.tiff (1x + 2x) from dmg-background.svg:
//   swift packaging/macos/render_dmg_background.swift packaging/macos
//   cd packaging/macos && tiffutil -cathidpicheck dmg-background.png dmg-background@2x.png \
//       -out dmg-background.tiff && rm dmg-background.png dmg-background@2x.png
import AppKit
let dir = URL(fileURLWithPath: CommandLine.arguments[1])
guard let image = NSImage(contentsOf: dir.appendingPathComponent("dmg-background.svg")) else { fatalError("cannot read the SVG") }
let points = NSSize(width: 2000, height: 1300)
func render(scale: Int, to name: String) {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(points.width) * scale, pixelsHigh: Int(points.height) * scale,
                               bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = points
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    NSGraphicsContext.current!.imageInterpolation = .high
    image.draw(in: NSRect(origin: .zero, size: points), from: .zero, operation: .copy, fraction: 1)
    NSGraphicsContext.restoreGraphicsState()
    try! rep.representation(using: .png, properties: [:])!.write(to: dir.appendingPathComponent(name))
}
render(scale: 1, to: "dmg-background.png")
render(scale: 2, to: "dmg-background@2x.png")
