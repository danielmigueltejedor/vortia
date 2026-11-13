import SwiftUI
import VortiaKit

/// What a click on the drawing does.
enum SketchTool: String, CaseIterable, Identifiable {
    /// Pick and drag vertices; drag the background to move the view.
    case select
    /// Click on a segment to put a vertex there.
    case addPoint
    /// Drag anywhere to move the view.
    case pan

    var id: String { rawValue }

    var symbol: String {
        switch self {
        case .select: "cursorarrow"
        case .addPoint: "plus.circle"
        case .pan: "hand.raised"
        }
    }

    var title: String {
        switch self {
        case .select: L10n.t(.toolSelect)
        case .addPoint: L10n.t(.toolAddPoint)
        case .pan: L10n.t(.toolPan)
        }
    }

    var hint: String {
        switch self {
        case .select: L10n.t(.toolSelectHint)
        case .addPoint: L10n.t(.toolAddPointHint)
        case .pan: L10n.t(.toolPanHint)
        }
    }
}

/// What is picked in the drawing.
enum SketchSelection: Equatable {
    case vertex(Int)
    case segment(Int)
}

/// The geometry stage of a contour project: the fluid contour drawn to scale on a grid, edited
/// in place. X is the axis of rotation and Y the radius, both in millimetres. Every change goes
/// to the engine and can be undone.
struct GeometryEditor: View {
    @Environment(VortiaStore.self) private var store
    @Binding var selection: SketchSelection?
    @State private var tool = SketchTool.select
    @State private var zoom = 1.0
    @State private var pan = CGSize.zero
    @State private var panStart: CGSize?
    @State private var dragging: (index: Int, point: [Double])?
    @State private var cursor: [Double]?
    @State private var showNozzle = false
    @GestureState private var pinch = 1.0

    var body: some View {
        VStack(spacing: 0) {
            ZStack(alignment: .top) {
                canvas
                toolbar.padding(.top, 12)
            }
            .canvasSurface()
            .padding([.horizontal, .top], 16)
            statusLine
                .padding(.horizontal, 22)
                .padding(.vertical, 10)
        }
        .disabled(store.job.isRunning)
    }

    // MARK: Drawing

    private func viewport(_ size: CGSize) -> ContourViewport {
        ContourViewport(points: store.sketch?.points ?? [], size: size, zoom: zoom * pinch, pan: pan)
    }

    /// The contour as it is drawn now: with the vertex being dragged where the pointer is.
    private var shape: VortiaStore.ContourShape? {
        guard var shape = store.sketch?.shape else { return nil }
        if let dragging, shape.points.indices.contains(dragging.index) {
            shape.points[dragging.index] = dragging.point
        }
        return shape
    }

    private var canvas: some View {
        GeometryReader { proxy in
            let viewport = viewport(proxy.size)
            Canvas { context, _ in draw(viewport, in: &context) }
                .contentShape(Rectangle())
                .gesture(drag(viewport))
                .simultaneousGesture(
                    MagnifyGesture()
                        .updating($pinch) { value, state, _ in state = value.magnification }
                        .onEnded { value in zoom = min(max(zoom * value.magnification, 0.2), 60) })
                .onContinuousHover { phase in
                    switch phase {
                    case .active(let location): cursor = viewport.model(location)
                    case .ended: cursor = nil
                    }
                }
                .focusable()
                .focusEffectDisabled()
                .onDeleteCommand { deleteSelection() }
                .accessibilityLabel(L10n.t(.geometry))
        }
        .frame(minWidth: 420, minHeight: 320)
    }

    private func draw(_ viewport: ContourViewport, in context: inout GraphicsContext) {
        ContourDrawing.grid(viewport, in: &context)
        guard let shape, shape.points.count >= 2 else { return }
        context.fill(ContourDrawing.outline(shape, viewport), with: .color(.accentColor.opacity(0.08)))
        for index in shape.points.indices {
            let selected = selection == .segment(index)
            context.stroke(
                ContourDrawing.segment(index, of: shape, viewport),
                with: .color(selected ? Color.accentColor : Color.primary),
                style: StrokeStyle(lineWidth: selected ? 3.5 : 1.8, lineCap: .round))
        }
        for (index, point) in shape.points.enumerated() {
            let p = viewport.screen(point)
            let selected = selection == .vertex(index)
            let radius = selected ? 5.5 : 3.5
            let dot = Path(ellipseIn: CGRect(x: p.x - radius, y: p.y - radius, width: radius * 2, height: radius * 2))
            context.fill(dot, with: .color(selected ? Color.accentColor : Color.primary))
            if selected {
                context.stroke(dot, with: .color(.white), lineWidth: 1.5)
            }
        }
        // The overall length, under the axis.
        let xs = shape.points.map { $0[safe: 0] }
        if let low = xs.min(), let high = xs.max(), high > low {
            let y = viewport.screen([0, 0]).y + 26
            let a = CGPoint(x: viewport.screen([low, 0]).x, y: y)
            let b = CGPoint(x: viewport.screen([high, 0]).x, y: y)
            var line = Path()
            line.move(to: a)
            line.addLine(to: b)
            for end in [a, b] {
                line.move(to: CGPoint(x: end.x, y: y - 5))
                line.addLine(to: CGPoint(x: end.x, y: y + 5))
            }
            context.stroke(line, with: .color(.secondary), lineWidth: 1)
            context.draw(
                Text(Measure.millimetres(high - low)).font(.caption.monospacedDigit()).foregroundStyle(.secondary),
                at: CGPoint(x: (a.x + b.x) / 2, y: y + 13))
        }
    }

    // MARK: Pointer

    private func drag(_ viewport: ContourViewport) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { value in
                guard let shape = store.sketch?.shape else { return }
                if dragging == nil, panStart == nil {
                    if tool == .select,
                        let index = ContourDrawing.vertex(near: value.startLocation, in: shape, viewport)
                    {
                        selection = .vertex(index)
                        dragging = (index, shape.points[index])
                    } else if tool != .addPoint {
                        panStart = pan
                    }
                }
                if let current = dragging {
                    guard hypot(value.translation.width, value.translation.height) > 2 else { return }
                    dragging = (current.index, snapped(viewport.model(value.location)))
                } else if let panStart {
                    pan = CGSize(
                        width: panStart.width + value.translation.width,
                        height: panStart.height + value.translation.height)
                }
            }
            .onEnded { value in
                defer {
                    dragging = nil
                    panStart = nil
                }
                guard let sketch = store.sketch else { return }
                let moved = hypot(value.translation.width, value.translation.height) > 2
                if let current = dragging, moved {
                    var points = sketch.points
                    points[current.index] = current.point
                    Task { await store.updateContour(points: points, closed: sketch.closed, bulges: sketch.bulges) }
                    return
                }
                guard !moved else { return }
                switch tool {
                case .select:
                    if let index = ContourDrawing.vertex(near: value.location, in: sketch.shape, viewport) {
                        selection = .vertex(index)
                    } else if let index = ContourDrawing.segment(near: value.location, in: sketch.shape, viewport) {
                        selection = .segment(index)
                    } else {
                        selection = nil
                    }
                case .addPoint:
                    guard let index = ContourDrawing.segment(near: value.location, in: sketch.shape, viewport, reach: 20)
                    else { return }
                    insertVertex(after: index, at: snapped(viewport.model(value.location)))
                case .pan:
                    break
                }
            }
    }

    /// A contour stays above the axis, and lands on it within half a millimetre; positions keep
    /// a hundredth of a millimetre.
    private func snapped(_ point: [Double]) -> [Double] {
        let x = (point[0] * 100).rounded() / 100
        let y = point[1] < 0.5 ? 0 : (point[1] * 100).rounded() / 100
        return [x, y]
    }

    private func insertVertex(after index: Int, at point: [Double]) {
        guard let sketch = store.sketch else { return }
        var points = sketch.points
        var bulges = sketch.bulges
        points.insert(point, at: index + 1)
        bulges[index] = 0
        bulges.insert(0, at: index + 1)
        selection = .vertex(index + 1)
        Task { await store.updateContour(points: points, closed: sketch.closed, bulges: bulges) }
    }

    private func deleteSelection() {
        guard let sketch = store.sketch, sketch.points.count > 3 else { return }
        let vertex: Int
        switch selection {
        case .vertex(let index): vertex = index
        case .segment(let index): vertex = (index + 1) % sketch.points.count
        case nil: return
        }
        GeometryEdits.removeVertex(vertex, from: sketch, store: store)
        selection = nil
    }

    // MARK: Controls

    private var toolbar: some View {
        HStack(spacing: 10) {
            FloatingBar {
                ForEach(SketchTool.allCases) { item in
                    Button {
                        tool = item
                    } label: {
                        Label(item.title, systemImage: item.symbol)
                            .frame(width: 28, height: 24)
                            .background(tool == item ? Color.accentColor.opacity(0.2) : .clear, in: Capsule())
                    }
                    .help(item.title)
                }
            }
            FloatingBar {
                Button { zoom = max(zoom / 1.4, 0.2) } label: {
                    Label(L10n.t(.zoomOut), systemImage: "minus.magnifyingglass")
                }
                .help(L10n.t(.zoomOut))
                Button { zoom = min(zoom * 1.4, 60) } label: {
                    Label(L10n.t(.zoomIn), systemImage: "plus.magnifyingglass")
                }
                .help(L10n.t(.zoomIn))
                Button {
                    zoom = 1
                    pan = .zero
                } label: {
                    Label(L10n.t(.fitView), systemImage: "arrow.up.left.and.down.right.and.arrow.up.right.and.down.left")
                }
                .help(L10n.t(.fitView))
            }
            FloatingBar {
                Button { Task { await store.undoContour() } } label: {
                    Label(L10n.t(.undo), systemImage: "arrow.uturn.backward")
                }
                .disabled(!store.canUndoContour)
                .help(L10n.t(.undo))
                Button { Task { await store.redoContour() } } label: {
                    Label(L10n.t(.redo), systemImage: "arrow.uturn.forward")
                }
                .disabled(!store.canRedoContour)
                .help(L10n.t(.redo))
            }
            FloatingBar {
                Button { showNozzle = true } label: {
                    Label(L10n.t(.nozzle), systemImage: "wand.and.stars")
                        .labelStyle(.titleAndIcon)
                }
                .popover(isPresented: $showNozzle, arrowEdge: .bottom) {
                    NozzleForm { showNozzle = false }
                        .frame(width: 340)
                }
            }
        }
    }

    private var statusLine: some View {
        HStack(spacing: 12) {
            if let sketch = store.sketch {
                Text(GeometryEdits.summary(sketch))
            }
            Spacer()
            Text(tool.hint).foregroundStyle(.tertiary)
            if let cursor {
                Text("X \(Measure.number(cursor[0])) · Y \(Measure.number(max(cursor[1], 0))) mm")
                    .monospacedDigit()
                    .frame(minWidth: 170, alignment: .trailing)
            }
        }
        .font(.callout)
        .foregroundStyle(.secondary)
        .lineLimit(1)
    }
}

/// The edits the editor and its inspector share.
@MainActor
enum GeometryEdits {
    /// "43 vertices · 43 segments · Perimeter: 1269.8 mm · Closed profile", as Vortia's status bar.
    static func summary(_ sketch: Sketch) -> String {
        [
            "\(sketch.points.count) \(L10n.t(.vertices).lowercased())",
            "\(sketch.segments.count) \(L10n.t(.segments).lowercased())",
            "\(L10n.t(.perimeter)): \(Measure.millimetres(ContourDrawing.perimeter(sketch)))",
            sketch.closed ? L10n.t(.closedProfile) : L10n.t(.openProfile),
        ].joined(separator: " · ")
    }

    /// Removes a vertex; its two segments become one straight one.
    static func removeVertex(_ vertex: Int, from sketch: Sketch, store: VortiaStore) {
        guard sketch.points.count > 3, sketch.points.indices.contains(vertex) else { return }
        var points = sketch.points
        var bulges = sketch.bulges
        points.remove(at: vertex)
        bulges.remove(at: vertex)
        bulges[(vertex - 1 + bulges.count) % bulges.count] = 0
        Task { await store.updateContour(points: points, closed: sketch.closed, bulges: bulges) }
    }

    static func move(_ vertex: Int, to point: [Double], in sketch: Sketch, store: VortiaStore) {
        guard sketch.points.indices.contains(vertex), point.allSatisfy(\.isFinite) else { return }
        var points = sketch.points
        points[vertex] = [point[0], max(point[1], 0)]
        Task { await store.updateContour(points: points, closed: sketch.closed, bulges: sketch.bulges) }
    }

    static func bend(_ segment: Int, to bulge: Double, in sketch: Sketch, store: VortiaStore) {
        guard sketch.bulges.indices.contains(segment) else { return }
        var bulges = sketch.bulges
        bulges[segment] = min(max(bulge, -1), 1)
        Task { await store.updateContour(points: sketch.points, closed: sketch.closed, bulges: bulges) }
    }

    /// Puts a vertex in the middle of a segment.
    static func split(_ segment: Int, in sketch: Sketch, store: VortiaStore) -> Int? {
        guard let (a, b) = sketch.ends(of: segment) else { return nil }
        var points = sketch.points
        var bulges = sketch.bulges
        points.insert([(a[0] + b[0]) / 2, (a[1] + b[1]) / 2], at: segment + 1)
        bulges[segment] = 0
        bulges.insert(0, at: segment + 1)
        Task { await store.updateContour(points: points, closed: sketch.closed, bulges: bulges) }
        return segment + 1
    }
}

/// The inspector of the geometry stage: what is selected, the vertex list and the nozzle.
struct GeometryInspector: View {
    @Environment(VortiaStore.self) private var store
    @Binding var selection: SketchSelection?

    var body: some View {
        Form {
            if let sketch = store.sketch {
                selectionSection(sketch)
                Section(L10n.t(.vertices)) {
                    Grid(alignment: .trailing, horizontalSpacing: 12, verticalSpacing: 4) {
                        GridRow {
                            Text("#").gridColumnAlignment(.leading)
                            Text("X (mm)")
                            Text("Y (mm)")
                        }
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                        ForEach(sketch.points.indices, id: \.self) { index in
                            GridRow {
                                Text("\(index + 1)").foregroundStyle(.secondary)
                                    .gridColumnAlignment(.leading)
                                Text(Measure.number(sketch.points[index][safe: 0]))
                                Text(Measure.number(sketch.points[index][safe: 1]))
                            }
                            .font(.callout.monospacedDigit())
                            .padding(.vertical, 1)
                            .background(
                                selection == .vertex(index) ? Color.accentColor.opacity(0.18) : .clear,
                                in: RoundedRectangle(cornerRadius: 4))
                            .contentShape(Rectangle())
                            .onTapGesture { selection = .vertex(index) }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .formStyle(.grouped)
        .disabled(store.job.isRunning)
    }

    @ViewBuilder private func selectionSection(_ sketch: Sketch) -> some View {
        switch selection {
        case .vertex(let index) where sketch.points.indices.contains(index):
            Section("\(L10n.t(.vertex)) \(index + 1)") {
                NumberField(
                    title: L10n.t(.coordinateX),
                    value: Binding(
                        get: { sketch.points[index][safe: 0] },
                        set: { GeometryEdits.move(index, to: [$0, sketch.points[index][safe: 1]], in: sketch, store: store) }))
                NumberField(
                    title: L10n.t(.coordinateY),
                    value: Binding(
                        get: { sketch.points[index][safe: 1] },
                        set: { GeometryEdits.move(index, to: [sketch.points[index][safe: 0], $0], in: sketch, store: store) }))
                Button(L10n.t(.deleteVertex), role: .destructive) {
                    GeometryEdits.removeVertex(index, from: sketch, store: store)
                    selection = nil
                }
                .disabled(sketch.points.count <= 3)
            }
        case .segment(let index) where sketch.segments.indices.contains(index):
            Section("\(L10n.t(.segment)) \(index + 1)") {
                LabeledContent(L10n.t(.length), value: Measure.millimetres(sketch.segments[index].length))
                if sketch.segments[index].axis {
                    Text(L10n.t(.onAxis)).font(.callout).foregroundStyle(.secondary)
                } else {
                    LabeledContent(L10n.t(.curvature)) {
                        Text(sketch.segments[index].bulge.formatted(.number.precision(.fractionLength(2))))
                            .monospacedDigit()
                    }
                    HStack {
                        Button(L10n.t(.straighten)) { GeometryEdits.bend(index, to: 0, in: sketch, store: store) }
                        Button(L10n.t(.bendOut)) {
                            GeometryEdits.bend(index, to: sketch.segments[index].bulge + 0.1, in: sketch, store: store)
                        }
                        Button(L10n.t(.bendIn)) {
                            GeometryEdits.bend(index, to: sketch.segments[index].bulge - 0.1, in: sketch, store: store)
                        }
                    }
                }
                Button(L10n.t(.splitSegment)) {
                    if let vertex = GeometryEdits.split(index, in: sketch, store: store) { selection = .vertex(vertex) }
                }
            }
        default:
            Section {
                Text(L10n.t(.contourHint)).font(.callout).foregroundStyle(.secondary)
            }
        }
    }
}

/// The five dimensions of the parametric nozzle, and the button that redraws the contour.
struct NozzleForm: View {
    @Environment(VortiaStore.self) private var store
    @State private var nozzle = NozzleDimensions()
    var done: () -> Void = {}

    var body: some View {
        Form {
            Section {
                NumberField(title: L10n.t(.inletDiameter), value: $nozzle.inletDiameterMm)
                NumberField(title: L10n.t(.throatDiameter), value: $nozzle.throatDiameterMm)
                NumberField(title: L10n.t(.outletDiameter), value: $nozzle.outletDiameterMm)
                NumberField(title: L10n.t(.convergingLength), value: $nozzle.convergingLengthMm)
                NumberField(title: L10n.t(.divergingLength), value: $nozzle.divergingLengthMm)
            } header: {
                Text(L10n.t(.nozzle))
            } footer: {
                Text(nozzle.isValid ? L10n.t(.nozzleReplaces) : L10n.t(.nozzleInvalid))
                    .foregroundStyle(nozzle.isValid ? Color.secondary : Color.red)
            }
            HStack {
                Spacer()
                Button(L10n.t(.makeNozzle)) {
                    Task { await store.makeNozzle(nozzle) }
                    done()
                }
                .buttonStyle(.borderedProminent)
                .disabled(!nozzle.isValid || store.job.isRunning)
            }
        }
        .formStyle(.grouped)
        .onAppear { if let n = store.sketch?.nozzle { nozzle = n } }
    }
}

/// Numbers as the interface writes them.
enum Measure {
    static func number(_ value: Double) -> String {
        guard value.isFinite else { return "—" }
        return value.formatted(.number.precision(.fractionLength(0...2)))
    }

    static func millimetres(_ value: Double) -> String {
        number(value) + " mm"
    }
}
