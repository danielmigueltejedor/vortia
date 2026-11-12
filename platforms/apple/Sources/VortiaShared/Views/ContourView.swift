import SwiftUI
import VortiaKit

/// The fluid contour of an axisymmetric case: X along the axis of rotation, Y the radius, in
/// millimetres. Drag a vertex to move it, pick a segment to bend it into an arc, or start again
/// from the parametric nozzle. Every change goes to the engine, which keeps the boundaries on
/// their segments.
struct ContourView: View {
    @Environment(VortiaStore.self) private var store
    @State private var nozzle = NozzleDimensions()
    @State private var selected: Int?
    @State private var dragging: (index: Int, point: [Double])?

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            canvas
                .frame(minWidth: 420, maxWidth: .infinity, maxHeight: .infinity)
                .card()
                .padding(16)
            Form { inspector }
                .formStyle(.grouped)
                .frame(width: 320)
        }
        .onAppear { if let n = store.sketch?.nozzle { nozzle = n } }
    }

    // MARK: Drawing

    private var canvas: some View {
        GeometryReader { proxy in
            let frame = Frame(sketch: store.sketch, size: proxy.size)
            ZStack {
                Canvas { context, _ in draw(frame, in: &context) }
                if let sketch = store.sketch {
                    ForEach(sketch.points.indices, id: \.self) { index in
                        let point = dragging?.index == index ? dragging!.point : sketch.points[index]
                        Circle()
                            .fill(Color.accentColor)
                            .frame(width: 10, height: 10)
                            .position(frame.screen(point))
                            .gesture(
                                DragGesture()
                                    .onChanged { value in
                                        dragging = (index, frame.model(value.location))
                                    }
                                    .onEnded { value in
                                        var points = sketch.points
                                        points[index] = frame.model(value.location)
                                        dragging = nil
                                        Task { await store.updateContour(points: points, closed: sketch.closed, bulges: sketch.bulges) }
                                    })
                            .help(L10n.t(.dragToMove))
                    }
                }
            }
            .contentShape(Rectangle())
            .onTapGesture { location in selected = frame.segment(near: location) }
        }
    }

    private func draw(_ frame: Frame, in context: inout GraphicsContext) {
        guard let sketch = store.sketch else { return }
        // The axis of rotation.
        var axis = Path()
        axis.move(to: frame.screen([frame.minX, 0]))
        axis.addLine(to: frame.screen([frame.maxX, 0]))
        context.stroke(axis, with: .color(.secondary), style: StrokeStyle(lineWidth: 1, dash: [6, 4]))

        var points = sketch.points
        if let dragging { points[dragging.index] = dragging.point }
        let types = store.flow?["boundaries"].array.map { $0["type"].string ?? "wall" } ?? []
        for index in sketch.segments.indices {
            guard points.count >= 2 else { break }
            let a = points[index], b = points[(index + 1) % points.count]
            let path = segmentPath(a, b, bulge: sketch.segments[index].bulge, frame: frame)
            let kind = types.indices.contains(index) ? types[index] : "wall"
            context.stroke(
                path, with: .color(BoundaryKind.color(kind)),
                lineWidth: selected == index ? 5 : 3)
            let middle = frame.screen([(a[0] + b[0]) / 2, (a[1] + b[1]) / 2])
            context.draw(
                Text("\(index + 1)").font(.caption2.weight(.bold)).foregroundStyle(.secondary),
                at: CGPoint(x: middle.x, y: middle.y - 10))
        }
    }

    /// A straight segment, or the circular arc its bulge describes (tan of a quarter of the sweep).
    private func segmentPath(_ a: [Double], _ b: [Double], bulge: Double, frame: Frame) -> Path {
        var path = Path()
        let p0 = frame.screen(a), p1 = frame.screen(b)
        path.move(to: p0)
        guard bulge != 0 else {
            path.addLine(to: p1)
            return path
        }
        let steps = 24
        let sweep = 4 * atan(bulge)
        let chord = hypot(b[0] - a[0], b[1] - a[1])
        let radius = chord / (2 * sin(abs(sweep) / 2))
        let mid = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2]
        let normal = [-(b[1] - a[1]) / chord, (b[0] - a[0]) / chord]
        let offset = radius * cos(sweep / 2) * (sweep > 0 ? 1 : -1)
        let centre = [mid[0] + normal[0] * offset, mid[1] + normal[1] * offset]
        let start = atan2(a[1] - centre[1], a[0] - centre[0])
        for step in 1...steps {
            let angle = start + sweep * Double(step) / Double(steps)
            path.addLine(to: frame.screen([centre[0] + radius * cos(angle), centre[1] + radius * sin(angle)]))
        }
        return path
    }

    // MARK: Inspector

    @ViewBuilder private var inspector: some View {
        Section(L10n.t(.nozzle)) {
            NumberField(title: L10n.t(.inletDiameter), value: $nozzle.inletDiameterMm)
            NumberField(title: L10n.t(.throatDiameter), value: $nozzle.throatDiameterMm)
            NumberField(title: L10n.t(.outletDiameter), value: $nozzle.outletDiameterMm)
            NumberField(title: L10n.t(.convergingLength), value: $nozzle.convergingLengthMm)
            NumberField(title: L10n.t(.divergingLength), value: $nozzle.divergingLengthMm)
            Button(L10n.t(.makeNozzle)) { Task { await store.makeNozzle(nozzle) } }
                .disabled(!nozzle.isValid || store.job.isRunning)
            if !nozzle.isValid {
                Text(L10n.t(.nozzleInvalid)).font(.caption).foregroundStyle(.red)
            }
        }
        if let sketch = store.sketch, let index = selected, sketch.segments.indices.contains(index) {
            Section("\(L10n.t(.segment)) \(index + 1)") {
                LabeledContent(L10n.t(.length), value: sketch.segments[index].length.scientific + " mm")
                if sketch.segments[index].axis {
                    Text(L10n.t(.onAxis)).font(.caption).foregroundStyle(.secondary)
                } else {
                    LabeledContent(L10n.t(.curvature), value: sketch.segments[index].bulge.formatted(.number.precision(.fractionLength(2))))
                    HStack {
                        Button(L10n.t(.straighten)) { bend(index, to: 0) }
                        Button(L10n.t(.bendOut)) { bend(index, to: sketch.segments[index].bulge + 0.1) }
                        Button(L10n.t(.bendIn)) { bend(index, to: sketch.segments[index].bulge - 0.1) }
                    }
                }
                Button(L10n.t(.removeVertex), role: .destructive) { removeVertex(index) }
                    .disabled(sketch.points.count <= 3)
            }
        } else {
            Section {
                Text(L10n.t(.contourHint)).font(.callout).foregroundStyle(.secondary)
            }
        }
    }

    private func bend(_ index: Int, to bulge: Double) {
        guard let sketch = store.sketch else { return }
        var bulges = sketch.bulges
        bulges[index] = min(max(bulge, -1), 1)
        Task { await store.updateContour(points: sketch.points, closed: sketch.closed, bulges: bulges) }
    }

    /// Removes the segment's end vertex; its two segments become one straight one.
    private func removeVertex(_ index: Int) {
        guard let sketch = store.sketch, sketch.points.count > 3 else { return }
        var points = sketch.points
        var bulges = sketch.bulges
        let vertex = (index + 1) % points.count
        points.remove(at: vertex)
        bulges.remove(at: vertex)
        bulges[(vertex - 1 + bulges.count) % bulges.count] = 0
        selected = nil
        Task { await store.updateContour(points: points, closed: sketch.closed, bulges: bulges) }
    }
}

/// Maps millimetres to the canvas and back, with the contour and the axis in view and Y up.
private struct Frame {
    var minX = 0.0, maxX = 1.0, minY = 0.0, maxY = 1.0
    var scale = 1.0
    var offset = CGPoint.zero
    var height = 0.0
    var sketch: Sketch?

    init(sketch: Sketch?, size: CGSize) {
        self.sketch = sketch
        let points = sketch?.points ?? [[0, 0], [100, 50]]
        minX = points.map { $0[0] }.min() ?? 0
        maxX = points.map { $0[0] }.max() ?? 1
        minY = min(0, points.map { $0[1] }.min() ?? 0)
        maxY = points.map { $0[1] }.max() ?? 1
        let margin = 36.0
        let width = max(maxX - minX, 1e-9), tall = max(maxY - minY, 1e-9)
        scale = min((size.width - 2 * margin) / width, (size.height - 2 * margin) / tall)
        offset = CGPoint(x: (size.width - width * scale) / 2, y: (size.height - tall * scale) / 2)
        height = size.height
    }

    func screen(_ p: [Double]) -> CGPoint {
        CGPoint(x: offset.x + (p[0] - minX) * scale, y: height - offset.y - (p[1] - minY) * scale)
    }

    func model(_ point: CGPoint) -> [Double] {
        let x = (point.x - offset.x) / scale + minX
        let y = (height - offset.y - point.y) / scale + minY
        // A contour stays above the axis, and on it within half a millimetre.
        return [(x * 1000).rounded() / 1000, max(0, y < 0.5 ? 0 : (y * 1000).rounded() / 1000)]
    }

    /// The segment whose midpoint is nearest, within 30 points.
    func segment(near location: CGPoint) -> Int? {
        guard let sketch, sketch.points.count >= 2 else { return nil }
        var best: (Int, Double)?
        for index in sketch.segments.indices {
            guard let (a, b) = sketch.ends(of: index) else { continue }
            let p = screen([(a[0] + b[0]) / 2, (a[1] + b[1]) / 2])
            let d = hypot(p.x - location.x, p.y - location.y)
            if d < 30, d < (best?.1 ?? .infinity) { best = (index, d) }
        }
        return best?.0
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
