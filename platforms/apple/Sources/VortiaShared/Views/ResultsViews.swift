import Charts
import ImageIO
import SwiftUI
import UniformTypeIdentifiers
import VortiaKit

// MARK: Field names

/// How a field is named and measured on screen, as Vortia names it.
enum FieldName {
    static func title(_ field: String) -> String {
        switch field {
        case "p": L10n.t(.fieldPressure) + " · p"
        case "p_rgh": L10n.t(.fieldPressure) + " · p_rgh"
        case "U": L10n.t(.fieldSpeed) + " · |U|"
        case "T": L10n.t(.fieldTemperature) + " · T"
        case "Mach", "Ma": L10n.t(.fieldMach)
        case "rho": L10n.t(.fieldDensity) + " · ρ"
        case "nut": L10n.t(.fieldTurbulentViscosity) + " · nut"
        case "nuTilda": "ν̃ · nuTilda"
        case "k": L10n.t(.fieldTurbulentEnergy) + " · k"
        case "omega": "ω · omega"
        case "alphat": "αt · alphat"
        default: field
        }
    }

    /// The unit, or an empty string for a number without one.
    static func unit(_ field: String, compressible: Bool) -> String {
        switch field {
        case "p", "p_rgh": compressible ? "Pa" : "m²/s²"
        case "U": "m/s"
        case "T": "K"
        case "rho": "kg/m³"
        case "nut", "nuTilda": "m²/s"
        case "k": "m²/s²"
        case "omega": "1/s"
        case "alphat": "kg/m/s"
        default: ""
        }
    }
}

// MARK: Viewer

/// The results stage: the latest solution filling the window, coloured by the chosen field,
/// with the colour bar beside it and the tools along the top, as in Vortia.
struct ResultsViewer: View {
    @Environment(VortiaStore.self) private var store
    @State private var probe: (location: CGPoint, point: [Double], value: Double)?
    @State private var probing = true
    @State private var exporting: ImageDocument?

    var body: some View {
        if let results = store.results {
            content(results)
        } else {
            EmptyState(
                symbol: "chart.xyaxis.line", title: L10n.t(.noResults),
                message: L10n.t(.noResultsBody)
            ) {
                Button(L10n.t(.goToCalculation)) { store.stage = .calculation }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .padding(.top, 6)
            }
        }
    }

    private func content(_ results: ResultsInfo) -> some View {
        ZStack(alignment: .top) {
            Group {
                if store.threeD {
                    if let surface = store.surface {
                        FieldScene(surface: surface).padding(16)
                    } else {
                        ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                } else {
                    plot
                }
            }
            .canvasSurface()
            .padding(16)
            tools(results).padding(.top, 28)
        }
        .fileExporter(
            isPresented: Binding(get: { exporting != nil }, set: { if !$0 { exporting = nil } }),
            document: exporting, contentType: .png,
            defaultFilename: "\(store.project?.name ?? "Vortia") \(store.field ?? "")"
        ) { _ in exporting = nil }
    }

    private var revolve: Bool { store.isFlowProject && store.sliceAxis == 2 && store.revolve }

    private var plot: some View {
        HStack(spacing: 0) {
            GeometryReader { proxy in
                if let slice = store.slice, let plotting = SlicePlot(slice: slice, axis: store.sliceAxis, mirror: revolve, size: proxy.size) {
                    Canvas { context, _ in plotting.draw(in: &context) }
                        .contentShape(Rectangle())
                        .onContinuousHover { phase in
                            guard probing, case .active(let location) = phase,
                                let found = plotting.probe(location)
                            else {
                                probe = nil
                                return
                            }
                            probe = (location, found.point, found.value)
                        }
                        .overlay(alignment: .topLeading) {
                            if let probe {
                                probeLabel(probe)
                                    .offset(x: min(probe.location.x + 14, proxy.size.width - 220), y: probe.location.y + 14)
                                    .allowsHitTesting(false)
                            }
                        }
                } else {
                    ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .padding(.top, 64)
            .padding([.leading, .bottom], 24)
            if let range = store.slice?.range, range.count == 2 {
                VerticalColorBar(
                    title: store.field.map(FieldName.title) ?? "",
                    unit: FieldName.unit(store.field ?? "", compressible: store.isFlowProject),
                    range: range[0]...max(range[1], range[0]))
                .frame(width: 96)
                .padding(.vertical, 80)
                .padding(.trailing, 16)
            }
        }
    }

    private func probeLabel(_ probe: (location: CGPoint, point: [Double], value: Double)) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("\(probe.value.scientific) \(FieldName.unit(store.field ?? "", compressible: store.isFlowProject))")
                .font(.callout.weight(.semibold).monospacedDigit())
            Text("X \(Measure.number(probe.point[0] * 1000)) · Y \(Measure.number(probe.point[1] * 1000)) mm")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
    }

    private func tools(_ results: ResultsInfo) -> some View {
        @Bindable var store = store
        return HStack(spacing: 10) {
            FloatingBar {
                Picker(L10n.t(.field), selection: $store.field) {
                    ForEach(results.fields, id: \.self) { Text(FieldName.title($0)).tag(Optional($0)) }
                }
                .labelsHidden()
                .frame(width: 210)
            }
            FloatingBar {
                Picker(L10n.t(.view), selection: $store.threeD) {
                    Label(L10n.t(.slice), systemImage: "square.split.diagonal").tag(false)
                    Label(L10n.t(.threeD), systemImage: "cube").tag(true)
                }
                .pickerStyle(.segmented)
                .labelStyle(.titleAndIcon)
                .labelsHidden()
                .frame(width: 210)
            }
            if !store.threeD {
                FloatingBar {
                    Menu {
                        Picker(L10n.t(.axis), selection: $store.sliceAxis) {
                            Text(L10n.t(.planeXY)).tag(2)
                            Text(L10n.t(.planeXZ)).tag(1)
                            Text(L10n.t(.planeYZ)).tag(0)
                        }
                        .pickerStyle(.inline)
                    } label: {
                        Label(L10n.t(.sections), systemImage: "square.split.bottomrightquarter")
                            .labelStyle(.titleAndIcon)
                    }
                    .menuStyle(.borderlessButton)
                    .fixedSize()
                    if !store.isFlowProject || store.sliceAxis != 2 {
                        Slider(value: $store.sliceFraction, in: 0...1) {
                            Text(L10n.t(.position))
                        } onEditingChanged: { editing in
                            if !editing { Task { await store.loadSlice() } }
                        }
                        .frame(width: 120)
                        .help(L10n.t(.position))
                    }
                    if store.isFlowProject, store.sliceAxis == 2 {
                        Toggle(isOn: $store.revolve) {
                            Label(L10n.t(.mirrorAxis), systemImage: "arrow.up.and.down.righttriangle.up.righttriangle.down")
                        }
                        .toggleStyle(.button)
                        .help(L10n.t(.mirrorAxis))
                    }
                    Toggle(isOn: $probing) {
                        Label(L10n.t(.probe), systemImage: "scope")
                    }
                    .toggleStyle(.button)
                    .help(L10n.t(.probeHelp))
                }
            }
            FloatingBar {
                Button {
                    exporting = imageDocument()
                } label: {
                    Label(L10n.t(.saveImage), systemImage: "square.and.arrow.down")
                        .labelStyle(.titleAndIcon)
                }
                .disabled(store.threeD || store.slice == nil)
            }
        }
    }

    /// The slice and its colour bar drawn again at a fixed size, for a picture.
    private func imageDocument() -> ImageDocument? {
        guard let slice = store.slice, let range = slice.range, range.count == 2 else { return nil }
        let size = CGSize(width: 1600, height: 900)
        let mirror = revolve
        let axis = store.sliceAxis
        let field = store.field ?? ""
        let picture = HStack(spacing: 0) {
            Canvas { context, canvasSize in
                SlicePlot(slice: slice, axis: axis, mirror: mirror, size: canvasSize)?.draw(in: &context)
            }
            .padding(40)
            VerticalColorBar(
                title: FieldName.title(field), unit: FieldName.unit(field, compressible: store.isFlowProject),
                range: range[0]...max(range[1], range[0])
            )
            .frame(width: 140)
            .padding(.vertical, 120)
            .padding(.trailing, 30)
        }
        .frame(width: size.width, height: size.height)
        .background(Color.white)
        .environment(\.colorScheme, .light)
        let renderer = ImageRenderer(content: picture)
        renderer.scale = 2
        guard let image = renderer.cgImage else { return nil }
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data as CFMutableData, UTType.png.identifier as CFString, 1, nil)
        else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else { return nil }
        return ImageDocument(data: data as Data)
    }
}

/// A PNG to save.
struct ImageDocument: FileDocument {
    static let readableContentTypes: [UTType] = [.png]
    var data: Data

    init(data: Data) { self.data = data }

    init(configuration: ReadConfiguration) throws {
        data = configuration.file.regularFileContents ?? Data()
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: data)
    }
}

/// A slice laid flat on a canvas: the axis it is normal to is dropped and the other two become
/// the picture's x and y, metres to points. A mirrored slice is drawn again below the axis, the
/// way an axisymmetric result is read.
struct SlicePlot {
    var slice: FieldTriangles
    var range: [Double]
    var a: Int
    var b: Int
    var mirror: Bool
    var box: CGRect
    var scale: Double
    var offset: CGPoint
    var height: Double

    init?(slice: FieldTriangles, axis: Int, mirror: Bool, size: CGSize) {
        guard slice.triangleCount > 0, let range = slice.range, range.count == 2,
            size.width > 10, size.height > 10
        else { return nil }
        self.slice = slice
        self.range = range
        self.mirror = mirror
        let plane = axis == 0 ? (1, 2) : axis == 1 ? (0, 2) : (0, 1)
        let a = plane.0, b = plane.1
        self.a = a
        self.b = b
        var minA = Float.infinity, maxA = -Float.infinity, minB = Float.infinity, maxB = -Float.infinity
        for vertex in stride(from: 0, to: slice.positions.count - 2, by: 3) {
            minA = min(minA, slice.positions[vertex + a])
            maxA = max(maxA, slice.positions[vertex + a])
            minB = min(minB, slice.positions[vertex + b])
            maxB = max(maxB, slice.positions[vertex + b])
        }
        if mirror { minB = min(minB, -maxB) }
        box = CGRect(
            x: Double(minA), y: Double(minB), width: Double(max(maxA - minA, 1e-12)),
            height: Double(max(maxB - minB, 1e-12)))
        scale = min(size.width / box.width, size.height / box.height)
        offset = CGPoint(x: (size.width - box.width * scale) / 2, y: (size.height - box.height * scale) / 2)
        height = size.height
    }

    private func point(_ vertex: Int, flip: Bool) -> CGPoint {
        let i = vertex * 3
        let u = Double(slice.positions[i + a])
        let v = Double(slice.positions[i + b]) * (flip ? -1 : 1)
        return CGPoint(x: offset.x + (u - box.minX) * scale, y: height - offset.y - (v - box.minY) * scale)
    }

    func draw(in context: inout GraphicsContext) {
        let span = max(range[1] - range[0], 1e-30)
        for flip in mirror ? [false, true] : [false] {
            for triangle in 0..<slice.triangleCount {
                let v = triangle * 3
                guard slice.values.count > v + 2 else { break }
                var path = Path()
                path.move(to: point(v, flip: flip))
                path.addLine(to: point(v + 1, flip: flip))
                path.addLine(to: point(v + 2, flip: flip))
                path.closeSubpath()
                let mean = Double(slice.values[v] + slice.values[v + 1] + slice.values[v + 2]) / 3
                let color = Colormap.color((mean - range[0]) / span)
                context.fill(path, with: .color(color))
                // A hairline of the same colour hides the seams antialiasing leaves between cells.
                context.stroke(path, with: .color(color), lineWidth: 0.6)
            }
        }
    }

    /// The value under a point of the canvas, interpolated inside its triangle, with the point
    /// in metres along the two plotted axes.
    func probe(_ location: CGPoint) -> (point: [Double], value: Double)? {
        let u = (location.x - offset.x) / scale + box.minX
        var v = (height - offset.y - location.y) / scale + box.minY
        if mirror, v < 0 { v = -v }
        for triangle in 0..<slice.triangleCount {
            let i = triangle * 3
            guard slice.values.count > i + 2 else { break }
            let p = (0..<3).map { k in
                (Double(slice.positions[(i + k) * 3 + a]), Double(slice.positions[(i + k) * 3 + b]))
            }
            let det = (p[1].1 - p[2].1) * (p[0].0 - p[2].0) + (p[2].0 - p[1].0) * (p[0].1 - p[2].1)
            guard abs(det) > 1e-30 else { continue }
            let l0 = ((p[1].1 - p[2].1) * (u - p[2].0) + (p[2].0 - p[1].0) * (v - p[2].1)) / det
            let l1 = ((p[2].1 - p[0].1) * (u - p[2].0) + (p[0].0 - p[2].0) * (v - p[2].1)) / det
            let l2 = 1 - l0 - l1
            guard l0 >= -1e-9, l1 >= -1e-9, l2 >= -1e-9 else { continue }
            let value = l0 * Double(slice.values[i]) + l1 * Double(slice.values[i + 1]) + l2 * Double(slice.values[i + 2])
            return ([u, v], value)
        }
        return nil
    }
}

/// The colour bar beside the result: the field's name and unit, and five values down its side.
struct VerticalColorBar: View {
    var title: String
    var unit: String
    var range: ClosedRange<Double>

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(.caption.weight(.semibold)).lineLimit(2)
            if !unit.isEmpty {
                Text(unit).font(.caption2).foregroundStyle(.secondary)
            }
            HStack(alignment: .top, spacing: 8) {
                LinearGradient(
                    colors: stride(from: 1.0, through: 0.0, by: -0.1).map(Colormap.color),
                    startPoint: .top, endPoint: .bottom
                )
                .frame(width: 14)
                .clipShape(RoundedRectangle(cornerRadius: 4))
                VStack(alignment: .leading) {
                    ForEach(0..<5, id: \.self) { index in
                        if index > 0 { Spacer(minLength: 0) }
                        let t = 1 - Double(index) / 4
                        Text((range.lowerBound + (range.upperBound - range.lowerBound) * t).scientific)
                    }
                }
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.secondary)
            }
        }
    }
}

// MARK: Inspector

/// The inspector of the results stage: the field's statistics, a profile, and the section
/// analysis.
struct ResultsInspector: View {
    @Environment(VortiaStore.self) private var store
    @State private var tab = Tab.statistics

    enum Tab: Hashable { case statistics, profile, analysis }

    var body: some View {
        VStack(spacing: 0) {
            Picker("", selection: $tab) {
                Text(L10n.t(.statistics)).tag(Tab.statistics)
                Text(L10n.t(.profileShort)).tag(Tab.profile)
                Text(L10n.t(.analysisShort)).tag(Tab.analysis)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(12)
            switch tab {
            case .statistics: statistics
            case .profile: profile
            case .analysis: EngineeringView()
            }
        }
    }

    @ViewBuilder private var statistics: some View {
        if let statistics = store.statistics {
            Form {
                Section(store.field.map(FieldName.title) ?? "") {
                    row(L10n.t(.minimum), statistics.minimum)
                    row(L10n.t(.maximum), statistics.maximum)
                    row(L10n.t(.mean), statistics.mean)
                    row(L10n.t(.standardDeviation), statistics.standardDeviation)
                    LabeledContent(L10n.t(.cells), value: statistics.cells.formatted())
                        .monospacedDigit()
                }
                if !statistics.histogram.isEmpty {
                    Section(L10n.t(.histogram)) {
                        Chart(Array(statistics.histogram.enumerated()), id: \.offset) { item in
                            BarMark(
                                x: .value(L10n.t(.value), item.offset),
                                y: .value(L10n.t(.cells), item.element.cells))
                            .foregroundStyle(
                                Colormap.color(Double(item.offset) / Double(max(statistics.histogram.count - 1, 1))))
                        }
                        .chartXAxis(.hidden)
                        .frame(height: 120)
                    }
                }
            }
            .formStyle(.grouped)
        } else {
            EmptyState(symbol: "chart.bar", title: L10n.t(.noResults))
        }
    }

    private func row(_ title: String, _ value: Double?) -> some View {
        LabeledContent(title, value: value?.scientific ?? "—").monospacedDigit()
    }

    private var profile: some View {
        @Bindable var store = store
        return Form {
            Section {
                Picker(L10n.t(.profileLine), selection: $store.profileLine) {
                    Text(L10n.t(.alongAxis)).tag(VortiaStore.ProfileLine.axis)
                    Text(L10n.t(.verticalCentre)).tag(VortiaStore.ProfileLine.vertical)
                }
                .pickerStyle(.radioGroup)
            }
            if let profile = store.profile {
                Section(store.field.map(FieldName.title) ?? "") {
                    ProfileChart(profile: profile, along: store.profileLine)
                        .frame(height: 260)
                }
            }
        }
        .formStyle(.grouped)
    }
}

/// A field along a line: plotted against the distance along X, or as a vertical profile with
/// the distance upwards.
struct ProfileChart: View {
    var profile: LineProfile
    var along: VortiaStore.ProfileLine

    var body: some View {
        Chart(Array(profile.samples.enumerated()), id: \.offset) { item in
            if let distance = item.element.distance, let value = item.element.value {
                if along == .axis {
                    LineMark(x: .value(L10n.t(.distance), distance), y: .value(L10n.t(.value), value))
                        .foregroundStyle(.tint)
                } else {
                    LineMark(x: .value(L10n.t(.value), value), y: .value(L10n.t(.distance), distance))
                        .foregroundStyle(.tint)
                }
            }
        }
    }
}
