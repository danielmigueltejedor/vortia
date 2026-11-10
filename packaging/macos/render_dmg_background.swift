// Draws the Vortia installer background (dark blue flow ribbons) at 1x and 2x.
//   swift packaging/macos/render_dmg_background.swift packaging/macos
//   cd packaging/macos && tiffutil -cathidpicheck dmg-background.png dmg-background@2x.png \
//       -out dmg-background.tiff && rm dmg-background.png dmg-background@2x.png
//
// The Finder window shows a 660x432 point area (icons at 160,190 and 500,190).
// The picture is 2000x1300 points: Finder never scales it, so a window opened
// larger keeps showing artwork instead of a white band.
import AppKit

let dir = URL(fileURLWithPath: CommandLine.arguments[1])
let size = CGSize(width: 2000, height: 1300)

func rgb(_ hex: UInt32, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(red: CGFloat((hex >> 16) & 255) / 255, green: CGFloat((hex >> 8) & 255) / 255,
            blue: CGFloat(hex & 255) / 255, alpha: alpha)
}

func draw(_ c: CGContext) {
    // Top-left origin, like the Finder window.
    c.translateBy(x: 0, y: size.height); c.scaleBy(x: 1, y: -1)
    c.setFillColor(rgb(0x030916)); c.fill(CGRect(origin: .zero, size: size))
    let space = CGColorSpaceCreateDeviceRGB()
    let glow = CGGradient(colorsSpace: space, colors: [rgb(0x0C3A9A, 0.55), rgb(0x061B4A, 0.25), rgb(0x030916, 0)] as CFArray,
                          locations: [0, 0.45, 1])!
    c.drawRadialGradient(glow, startCenter: CGPoint(x: 350, y: 380), startRadius: 0,
                         endCenter: CGPoint(x: 350, y: 380), endRadius: 520, options: [])
    c.drawRadialGradient(glow, startCenter: CGPoint(x: 1250, y: 620), startRadius: 0,
                         endCenter: CGPoint(x: 1250, y: 620), endRadius: 700, options: [])

    // A family of ribbons: down into a valley under the icons, up to the right,
    // then drifting on across the rest of the canvas.
    let ribbons = 46
    for i in 0..<ribbons {
        let t = CGFloat(i) / CGFloat(ribbons - 1)
        let wobble = sin(t * 9.0) * 10
        let path = CGMutablePath()
        path.move(to: CGPoint(x: -60, y: 70 + t * 210))
        path.addCurve(to: CGPoint(x: 345 + t * 50, y: 372 - t * 44 + wobble),
                      control1: CGPoint(x: 150, y: 150 + t * 170), control2: CGPoint(x: 235, y: 420 - t * 40))
        path.addCurve(to: CGPoint(x: 700 + t * 60, y: 150 + t * 150),
                      control1: CGPoint(x: 455 + t * 30, y: 325 - t * 70), control2: CGPoint(x: 520 - t * 30, y: 90 + t * 120))
        path.addCurve(to: CGPoint(x: 1300 + t * 80, y: 520 + t * 260),
                      control1: CGPoint(x: 880, y: 200 + t * 190), control2: CGPoint(x: 1020 + t * 60, y: 640 - t * 120))
        path.addCurve(to: CGPoint(x: 2080, y: 420 + t * 520),
                      control1: CGPoint(x: 1560, y: 430 + t * 380), control2: CGPoint(x: 1800, y: 760 - t * 80))
        let centre = 1 - abs(t - 0.5) * 2 // brightest ribbons in the middle of the family
        c.saveGState()
        c.setShadow(offset: .zero, blur: 9, color: rgb(0x2F8BFF, 0.55 * centre + 0.1))
        c.addPath(path)
        c.setStrokeColor(rgb(i % 5 == 0 ? 0x7DC1FF : 0x2A7BF0, 0.07 + 0.30 * centre * centre))
        c.setLineWidth(i % 5 == 0 ? 1.3 : 0.8)
        c.setLineCap(.round)
        c.strokePath()
        c.restoreGState()
    }
    // Darken the far edges so the icons' area stays the focus.
    let vignette = CGGradient(colorsSpace: space, colors: [rgb(0x030916, 0), rgb(0x030916, 0.55)] as CFArray, locations: [0, 1])!
    c.drawLinearGradient(vignette, start: CGPoint(x: 0, y: 0), end: CGPoint(x: 0, y: 44), options: [])
    c.drawLinearGradient(vignette, start: CGPoint(x: 0, y: 44), end: CGPoint(x: 0, y: 0), options: [])

    // Finder draws icon names in black over a picture: give them a light plate.
    for (x, width) in [(CGFloat(160), CGFloat(86)), (CGFloat(500), CGFloat(128))] {
        let plate = CGRect(x: x - width / 2, y: 262, width: width, height: 22)
        c.addPath(CGPath(roundedRect: plate, cornerWidth: 11, cornerHeight: 11, transform: nil))
        c.setFillColor(rgb(0xEAF2FF, 0.86)); c.fillPath()
    }
    // Arrow between the two icons.
    c.setStrokeColor(rgb(0x9CCBFF, 0.85)); c.setLineWidth(2.5); c.setLineCap(.round); c.setLineJoin(.round)
    c.move(to: CGPoint(x: 282, y: 190)); c.addLine(to: CGPoint(x: 378, y: 190))
    c.move(to: CGPoint(x: 366, y: 178)); c.addLine(to: CGPoint(x: 380, y: 190)); c.addLine(to: CGPoint(x: 366, y: 202))
    c.strokePath()
}

func render(scale: Int, to name: String) {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size.width) * scale, pixelsHigh: Int(size.height) * scale,
                               bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = size
    let context = NSGraphicsContext(bitmapImageRep: rep)!
    draw(context.cgContext)
    try! rep.representation(using: .png, properties: [:])!.write(to: dir.appendingPathComponent(name))
}
render(scale: 1, to: "dmg-background.png")
render(scale: 2, to: "dmg-background@2x.png")
