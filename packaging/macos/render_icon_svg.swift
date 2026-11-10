// Regenerate resources/icons/vortia.png from the vector master:
//   swift packaging/macos/render_icon_svg.swift resources/icons/vortia.svg resources/icons/vortia.png 512

import AppKit
let a = CommandLine.arguments
guard a.count == 4, let size = Int(a[3]), let image = NSImage(contentsOf: URL(fileURLWithPath: a[1])) else { fatalError("usage: render in.svg out.png size") }
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
rep.size = NSSize(width: size, height: size)
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
NSGraphicsContext.current!.imageInterpolation = .high
image.draw(in: NSRect(x: 0, y: 0, width: size, height: size), from: .zero, operation: .copy, fraction: 1)
NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: a[2]))
