import SwiftUI
import VortiaKit

/// Millimetres on a canvas and back: the contour fitted with a margin and Y up, then zoomed
/// about the middle of the canvas and panned. Every stage that draws the contour uses it, so the
/// shape sits in the same place as the person moves between them.
struct ContourViewport {
    var size: CGSize
    /// Points per millimetre.
    var scale: Double
    /// Where the model's origin lands on the canvas.
    var origin: CGPoint

    init(points: [[Double]], size: CGSize, zoom: Double = 1, pan: CGSize = .zero, margin: Double = 56) {
        self.size = size
        let xs = points.map { $0[safe: 0] }, ys = points.map { $0[safe: 1] }
        let minX = xs.min() ?? 0, maxX = xs.max() ?? 100
        let minY = min(0, ys.min() ?? 0), maxY = max(ys.max() ?? 50, minY + 1e-9)
        let width = max(maxX - minX, 1e-9), height = max(maxY - minY, 1e-9)
        let fit = min((size.width - 2 * margin) / width, (size.height - 2 * margin) / height)
        scale = max(fit, 1e-9) * zoom
        let centre = CGPoint(x: (minX + maxX) / 2, y: (minY + maxY) / 2)
        origin = CGPoint(
            x: size.width / 2 - centre.x * scale + pan.width,
            y: size.height / 2 + centre.y * scale + pan.height)
    }

    func screen(_ p: [Double]) -> CGPoint {
        CGPoint(x: origin.x + p[safe: 0] * scale, y: origin.y - p[safe: 1] * scale)
    }

    func model(_ q: CGPoint) -> [Double] {
        [(q.x - origin.x) / scale, (origin.y - q.y) / scale]
    }

    /// A round grid spacing (1, 2 or 5 × 10ⁿ mm) that lands between 36 and 90 points apart.
    var gridSpacing: Double {
        let target = 48 / max(scale, 1e-12)
        let power = pow(10, floor(log10(target)))
        for step in [1.0, 2, 5, 10] where step * power * scale >= 36 { return step * power }
        return 10 * power
    }
}

/// How the contour is drawn wherever it appears.
enum ContourDrawing {
    /// The faint millimetre grid, with every fifth line a little stronger, and the axis.
    static func grid(_ viewport: ContourViewport, in context: inout GraphicsContext) {
        let step = viewport.gridSpacing
        let low = viewport.model(.zero), high = viewport.model(CGPoint(x: viewport.size.width, y: viewport.size.height))
        let firstX = Int(floor(low[0] / step)), lastX = Int(ceil(high[0] / step))
        let firstY = Int(floor(high[1] / step)), lastY = Int(ceil(low[1] / step))
        guard lastX - firstX < 400, lastY - firstY < 400 else { return }
        for i in firstX...lastX {
            let x = viewport.screen([Double(i) * step, 0]).x
            var line = Path()
            line.move(to: CGPoint(x: x, y: 0))
            line.addLine(to: CGPoint(x: x, y: viewport.size.height))
            context.stroke(line, with: .color(.secondary.opacity(i % 5 == 0 ? 0.22 : 0.1)), lineWidth: 0.5)
        }
        for j in firstY...lastY {
            let y = viewport.screen([0, Double(j) * step]).y
            var line = Path()
            line.move(to: CGPoint(x: 0, y: y))
            line.addLine(to: CGPoint(x: viewport.size.width, y: y))
            context.stroke(line, with: .color(.secondary.opacity(j % 5 == 0 ? 0.22 : 0.1)), lineWidth: 0.5)
        }
        // The axis of rotation, Y = 0.
        var axis = Path()
        let y = viewport.screen([0, 0]).y
        axis.move(to: CGPoint(x: 0, y: y))
        axis.addLine(to: CGPoint(x: viewport.size.width, y: y))
        context.stroke(axis, with: .color(.secondary.opacity(0.7)), style: StrokeStyle(lineWidth: 1, dash: [8, 5]))
    }

    /// The whole contour as one closed path, for filling.
    static func outline(_ shape: VortiaStore.ContourShape, _ viewport: ContourViewport) -> Path {
        var path = Path()
        guard let first = shape.points.first else { return path }
        path.move(to: viewport.screen(first))
        for index in shape.points.indices {
            let a = shape.points[index], b = shape.points[(index + 1) % shape.points.count]
            appendSegment(a, b, bulge: shape.bulges[safe: index], viewport: viewport, to: &path)
        }
        path.closeSubpath()
        return path
    }

    static func segment(
        _ index: Int, of shape: VortiaStore.ContourShape, _ viewport: ContourViewport
    ) -> Path {
        var path = Path()
        guard shape.points.count >= 2, shape.points.indices.contains(index) else { return path }
        let a = shape.points[index], b = shape.points[(index + 1) % shape.points.count]
        path.move(to: viewport.screen(a))
        appendSegment(a, b, bulge: shape.bulges[safe: index], viewport: viewport, to: &path)
        return path
    }

    /// A straight segment, or the circular arc its bulge describes (tan of a quarter of the sweep).
    private static func appendSegment(
        _ a: [Double], _ b: [Double], bulge: Double, viewport: ContourViewport, to path: inout Path
    ) {
        let chord = hypot(b[safe: 0] - a[safe: 0], b[safe: 1] - a[safe: 1])
        guard bulge != 0, chord > 0 else {
            path.addLine(to: viewport.screen(b))
            return
        }
        let sweep = 4 * atan(bulge)
        let radius = chord / (2 * sin(abs(sweep) / 2))
        let mid = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2]
        let normal = [-(b[1] - a[1]) / chord, (b[0] - a[0]) / chord]
        let offset = radius * cos(sweep / 2) * (sweep > 0 ? 1 : -1)
        let centre = [mid[0] + normal[0] * offset, mid[1] + normal[1] * offset]
        let start = atan2(a[1] - centre[1], a[0] - centre[0])
        let steps = 24
        for step in 1...steps {
            let angle = start + sweep * Double(step) / Double(steps)
            path.addLine(to: viewport.screen([centre[0] + radius * cos(angle), centre[1] + radius * sin(angle)]))
        }
    }

    /// The segment nearest to a point on the canvas, within `reach` points.
    static func segment(
        near location: CGPoint, in shape: VortiaStore.ContourShape, _ viewport: ContourViewport,
        reach: Double = 14
    ) -> Int? {
        var best: (Int, Double)?
        for index in shape.points.indices {
            let a = viewport.screen(shape.points[index])
            let b = viewport.screen(shape.points[(index + 1) % shape.points.count])
            let d = distance(from: location, toSegment: a, b)
            if d < reach, d < (best?.1 ?? .infinity) { best = (index, d) }
        }
        return best?.0
    }

    /// The vertex nearest to a point on the canvas, within `reach` points.
    static func vertex(
        near location: CGPoint, in shape: VortiaStore.ContourShape, _ viewport: ContourViewport,
        reach: Double = 10
    ) -> Int? {
        var best: (Int, Double)?
        for (index, point) in shape.points.enumerated() {
            let p = viewport.screen(point)
            let d = hypot(p.x - location.x, p.y - location.y)
            if d < reach, d < (best?.1 ?? .infinity) { best = (index, d) }
        }
        return best?.0
    }

    static func distance(from p: CGPoint, toSegment a: CGPoint, _ b: CGPoint) -> Double {
        let dx = b.x - a.x, dy = b.y - a.y
        let length = dx * dx + dy * dy
        guard length > 0 else { return hypot(p.x - a.x, p.y - a.y) }
        let t = min(max(((p.x - a.x) * dx + (p.y - a.y) * dy) / length, 0), 1)
        return hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy))
    }

    /// The perimeter of the contour, in millimetres.
    static func perimeter(_ sketch: Sketch) -> Double {
        sketch.segments.reduce(0) { $0 + $1.length }
    }
}

extension Sketch {
    var shape: VortiaStore.ContourShape {
        VortiaStore.ContourShape(points: points, bulges: bulges)
    }
}

/// The colours boundary kinds are drawn in, the same everywhere they appear.
enum BoundaryKind {
    static let all = ["wall", "axis", "inlet", "outlet", "reservoir", "unassigned"]

    static func color(_ kind: String) -> Color {
        switch kind {
        case "inlet": .blue
        case "outlet": .orange
        case "axis": .secondary
        case "reservoir": .teal
        case "unassigned": .red
        default: .primary
        }
    }

    static func title(_ kind: String) -> String {
        switch kind {
        case "inlet": L10n.t(.inlet)
        case "outlet": L10n.t(.outlet)
        case "axis": L10n.t(.axisBoundary)
        case "reservoir": L10n.t(.reservoir)
        case "unassigned": L10n.t(.unassigned)
        default: L10n.t(.wall)
        }
    }
}

/// A rounded bar of controls floating over a canvas: the system's glass on macOS 26, a thick
/// material before.
struct FloatingBar<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        let bar = HStack(spacing: 4) { content }
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .buttonStyle(.borderless)
            .labelStyle(.iconOnly)
            .controlSize(.regular)
        #if compiler(>=6.2)
            if #available(macOS 26.0, *) {
                bar.glassEffect(.regular, in: Capsule())
            } else {
                bar.background(.regularMaterial, in: Capsule())
            }
        #else
            bar.background(.regularMaterial, in: Capsule())
        #endif
    }
}
