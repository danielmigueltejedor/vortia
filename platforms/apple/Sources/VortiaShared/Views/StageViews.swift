import SwiftUI
import VortiaKit

// MARK: Flow setup bindings

/// Bindings into the compressible setup the engine keeps; every change is saved after a pause.
@MainActor
enum FlowBinding {
    static func number(_ store: VortiaStore, _ section: String, _ key: String) -> Binding<Double> {
        Binding(
            get: { store.flow?[section][key].double ?? 0 },
            set: { store.flow?[section][key] = .number($0) })
    }

    static func text(_ store: VortiaStore, _ section: String, _ key: String, fallback: String) -> Binding<String> {
        Binding(
            get: { store.flow?[section][key].string ?? fallback },
            set: { store.flow?[section][key] = .string($0) })
    }

    static func criterion(_ store: VortiaStore, _ key: String) -> Binding<Double> {
        Binding(
            get: { store.flow?["solver"]["convergence"][key].double ?? 0 },
            set: { store.flow?["solver"]["convergence"][key] = .number($0) })
    }

    static func boundaryKinds(_ store: VortiaStore) -> [String] {
        store.flow?["boundaries"].array.map { $0["type"].string ?? "wall" } ?? []
    }
}

// MARK: Project

/// The project at a glance: where it lives, how far the case has come, and what to do next.
/// Each step opens its stage.
struct ProjectOverview: View {
    @Environment(VortiaStore.self) private var store

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                header
                if let next = nextStep {
                    nextCard(next)
                }
                VStack(spacing: 0) {
                    ForEach(Array(steps.enumerated()), id: \.offset) { index, step in
                        if index > 0 { Divider().padding(.leading, 52) }
                        stepRow(step)
                    }
                }
                .card(padding: 6)
                summary.card()
            }
            .padding(24)
            .frame(maxWidth: 760, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(store.project?.name ?? "").font(.largeTitle.weight(.bold))
            Text(store.project?.root ?? "")
                .font(.callout)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
                .lineLimit(1)
                .truncationMode(.middle)
        }
    }

    private struct Step {
        var stage: VortiaStore.Stage
        var detail: String
        var done: Bool
    }

    private var steps: [Step] {
        let state = store.project?.state ?? ""
        let meshed = state == "meshed" || state == "solved"
        var list: [Step] = []
        if let sketch = store.sketch {
            list.append(Step(stage: .geometry, detail: GeometryEdits.summary(sketch), done: sketch.closed))
            let size = store.flow?["mesh"]["sizeMm"].double ?? 0
            list.append(Step(
                stage: .mesh,
                detail: "\(L10n.t(.globalSize)): \(Measure.number(size)) · " + (meshed ? L10n.t(.meshReady) : L10n.t(.meshPending)),
                done: meshed))
            let kinds = FlowBinding.boundaryKinds(store)
            let open = kinds.filter { $0 == "inlet" || $0 == "outlet" || $0 == "reservoir" }.count
            list.append(Step(
                stage: .physics,
                detail: modelTitle + " · \(open) \(L10n.t(.openBoundaries))",
                done: !kinds.contains("unassigned") && open > 0))
        } else if let setup = store.setup {
            list.append(Step(
                stage: .geometry,
                detail: setup.size.map { Measure.number($0 * 1000) }.joined(separator: " × ") + " mm",
                done: true))
            list.append(Step(
                stage: .mesh, detail: "\(setup.mesh.cells.reduce(1, *).formatted()) \(L10n.t(.cells).lowercased())",
                done: meshed))
            list.append(Step(stage: .physics, detail: setup.material.name, done: true))
        }
        list.append(Step(stage: .calculation, detail: calculationDetail, done: state == "solved"))
        list.append(Step(
            stage: .results,
            detail: store.results.map { $0.fields.joined(separator: ", ") } ?? L10n.t(.noResults),
            done: store.results != nil))
        return list
    }

    private var modelTitle: String {
        switch store.flow?["material"]["turbulence"].string ?? "SpalartAllmaras" {
        case "laminar": L10n.t(.laminar)
        case "inviscid": L10n.t(.inviscid)
        default: "Spalart–Allmaras"
        }
    }

    private var calculationDetail: String {
        switch store.job {
        case .idle: store.project?.state == "solved" ? L10n.t(.finished) : L10n.t(.idle)
        case .running(let phase): phase == "mesh" ? L10n.t(.meshing) : L10n.t(.solving)
        case .finished(_, let ok, let summary): (ok ? L10n.t(.finished) : L10n.t(.failed)) + (summary.isEmpty ? "" : " · " + summary)
        }
    }

    private var nextStep: Step? {
        steps.first { !$0.done }
    }

    private func nextCard(_ step: Step) -> some View {
        HStack(spacing: 14) {
            Image(systemName: step.stage.symbol)
                .font(.title2)
                .foregroundStyle(.tint)
                .frame(width: 32)
            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.t(.nextStep)).font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                Text(step.stage.guidance).font(.headline)
            }
            Spacer()
            Button(step.stage.title) { store.stage = step.stage }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
        }
        .card()
    }

    private func stepRow(_ step: Step) -> some View {
        Button {
            store.stage = step.stage
        } label: {
            HStack(spacing: 14) {
                Image(systemName: step.stage.symbol)
                    .font(.title3)
                    .foregroundStyle(.secondary)
                    .frame(width: 28)
                VStack(alignment: .leading, spacing: 2) {
                    Text(step.stage.title).font(.body.weight(.semibold))
                    Text(step.detail).font(.callout).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer()
                Image(systemName: step.done ? "checkmark.circle.fill" : "circle")
                    .foregroundStyle(step.done ? Color.green : Color.secondary)
                Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private var summary: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.t(.engineAndSolver)).font(.headline)
            LabeledContent(L10n.t(.openFoam)) {
                if let openFoam = store.openFoam, openFoam.found {
                    Text(openFoam.fingerprint ?? "").foregroundStyle(.secondary)
                } else {
                    Text(L10n.t(.notFound)).foregroundStyle(.red)
                }
            }
            if !store.engineVersion.isEmpty {
                LabeledContent("vortia-engine", value: store.engineVersion)
            }
            if let file = store.results?.file {
                LabeledContent(L10n.t(.resultData)) {
                    Text(URL(fileURLWithPath: file).lastPathComponent).foregroundStyle(.secondary)
                }
            }
        }
    }
}

// MARK: Mesh

/// The mesh stage of a contour project: the contour with the sizes the mesher will aim for
/// drawn over it, and an estimate of how many cells that makes.
struct MeshStage: View {
    @Environment(VortiaStore.self) private var store

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            GeometryReader { proxy in
                Canvas { context, size in draw(in: &context, size: size) }
                    .frame(width: proxy.size.width, height: proxy.size.height)
            }
            .frame(minHeight: 320)
            .canvasSurface()
            .padding([.horizontal, .top], 16)
            HStack(spacing: 12) {
                Text(estimate)
                Spacer()
                Text(L10n.t(.meshSketchNote)).foregroundStyle(.tertiary)
            }
            .font(.callout)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 22)
            .padding(.vertical, 10)
        }
    }

    private var sizes: (global: Double, wall: Double) {
        let global = max(store.flow?["mesh"]["sizeMm"].double ?? 20, 1e-3)
        return (global, min(max(store.flow?["mesh"]["wallSizeMm"].double ?? 2, 1e-3), global))
    }

    /// Area over the square of the global size: an order of magnitude, not the mesher's count.
    private var estimate: String {
        guard let sketch = store.sketch else { return "" }
        var area = 0.0
        for i in sketch.points.indices {
            let a = sketch.points[i], b = sketch.points[(i + 1) % sketch.points.count]
            area += a[safe: 0] * b[safe: 1] - b[safe: 0] * a[safe: 1]
        }
        let cells = abs(area) / 2 / (sizes.global * sizes.global)
        return "≈ \(Int(cells.rounded()).formatted()) \(L10n.t(.cells).lowercased()) · \(L10n.t(.estimate))"
    }

    private func draw(in context: inout GraphicsContext, size: CGSize) {
        guard let sketch = store.sketch else { return }
        let viewport = ContourViewport(points: sketch.points, size: size)
        let outline = ContourDrawing.outline(sketch.shape, viewport)
        context.fill(outline, with: .color(.accentColor.opacity(0.06)))
        // Cells of the global size, clipped to the fluid.
        var inner = context
        inner.clip(to: outline)
        let step = sizes.global
        let xs = sketch.points.map { $0[safe: 0] }, ys = sketch.points.map { $0[safe: 1] }
        if let minX = xs.min(), let maxX = xs.max(), let maxY = ys.max(),
            step * viewport.scale >= 2, (maxX - minX) / step < 2000, maxY / step < 2000
        {
            var grid = Path()
            for x in stride(from: minX, through: maxX, by: step) {
                grid.move(to: viewport.screen([x, 0]))
                grid.addLine(to: viewport.screen([x, maxY]))
            }
            for y in stride(from: 0, through: maxY, by: step) {
                grid.move(to: viewport.screen([minX, y]))
                grid.addLine(to: viewport.screen([maxX, y]))
            }
            inner.stroke(grid, with: .color(.secondary.opacity(0.45)), lineWidth: 0.6)
        }
        // The refined band along the walls.
        let kinds = FlowBinding.boundaryKinds(store)
        for index in sketch.points.indices where (kinds.indices.contains(index) ? kinds[index] : "wall") == "wall" {
            inner.stroke(
                ContourDrawing.segment(index, of: sketch.shape, viewport),
                with: .color(.accentColor.opacity(0.35)),
                lineWidth: max(sizes.wall * 6 * viewport.scale, 3))
        }
        for index in sketch.points.indices {
            let kind = kinds.indices.contains(index) ? kinds[index] : "wall"
            context.stroke(
                ContourDrawing.segment(index, of: sketch.shape, viewport),
                with: .color(BoundaryKind.color(kind)), lineWidth: 2)
        }
    }
}

/// The mesh sizes, in the inspector.
struct MeshInspector: View {
    @Environment(VortiaStore.self) private var store

    var body: some View {
        Form {
            Section {
                NumberField(title: L10n.t(.globalSize), value: FlowBinding.number(store, "mesh", "sizeMm"))
                NumberField(title: L10n.t(.wallSize), value: FlowBinding.number(store, "mesh", "wallSizeMm"))
                NumberField(title: L10n.t(.wedgeAngle), value: FlowBinding.number(store, "mesh", "wedgeAngle"))
            } header: {
                Text(L10n.t(.mesh))
            } footer: {
                Text(L10n.t(.meshHint)).foregroundStyle(.secondary)
            }
            Section {
                Button {
                    Task { await store.mesh() }
                } label: {
                    Label(L10n.t(.generateMesh), systemImage: "square.grid.3x3")
                }
                .buttonStyle(.borderedProminent)
                .disabled(!store.canRun)
            }
        }
        .formStyle(.grouped)
        .disabled(store.job.isRunning)
    }
}

// MARK: Physics

/// The physics stage of a contour project: each segment coloured by its boundary, picked by
/// clicking it, and the gas, its model and the initial state.
struct PhysicsStage: View {
    @Environment(VortiaStore.self) private var store
    @Binding var boundary: Int?

    var body: some View {
        VStack(spacing: 0) {
            GeometryReader { proxy in
                let viewport = ContourViewport(points: store.sketch?.points ?? [], size: proxy.size)
                Canvas { context, _ in draw(viewport, in: &context) }
                    .contentShape(Rectangle())
                    .onTapGesture { location in
                        guard let shape = store.sketch?.shape else { return }
                        boundary = ContourDrawing.segment(near: location, in: shape, viewport, reach: 18)
                    }
            }
            .frame(minHeight: 260)
            .canvasSurface()
            .overlay(alignment: .bottomLeading) { legend.padding(12) }
            .padding([.horizontal, .top], 16)
            Form {
                Section(L10n.t(.gasModel)) {
                    Picker(L10n.t(.flowModel), selection: FlowBinding.text(store, "material", "turbulence", fallback: "SpalartAllmaras")) {
                        Text("Spalart–Allmaras").tag("SpalartAllmaras")
                        Text(L10n.t(.laminar)).tag("laminar")
                        Text(L10n.t(.inviscid)).tag("inviscid")
                    }
                    NumberField(title: L10n.t(.molecularWeight), value: FlowBinding.number(store, "material", "molecularWeight"))
                    NumberField(title: L10n.t(.specificHeat), value: FlowBinding.number(store, "material", "Cp"))
                    NumberField(title: L10n.t(.dynamicViscosity), value: FlowBinding.number(store, "material", "mu"))
                    NumberField(title: L10n.t(.conductivity), value: FlowBinding.number(store, "material", "conductivity"))
                }
                Section(L10n.t(.initialField)) {
                    NumberField(title: L10n.t(.initialPressure), value: FlowBinding.number(store, "initial", "pressure"))
                    NumberField(title: L10n.t(.initialTemperature), value: FlowBinding.number(store, "initial", "temperature"))
                }
            }
            .formStyle(.grouped)
            .frame(maxHeight: 380)
            .disabled(store.job.isRunning)
        }
    }

    private func draw(_ viewport: ContourViewport, in context: inout GraphicsContext) {
        guard let sketch = store.sketch else { return }
        context.fill(ContourDrawing.outline(sketch.shape, viewport), with: .color(.accentColor.opacity(0.06)))
        let kinds = FlowBinding.boundaryKinds(store)
        for index in sketch.points.indices {
            let kind = kinds.indices.contains(index) ? kinds[index] : "wall"
            let selected = boundary == index
            context.stroke(
                ContourDrawing.segment(index, of: sketch.shape, viewport),
                with: .color(BoundaryKind.color(kind)),
                style: StrokeStyle(lineWidth: selected ? 6 : kind == "wall" ? 2 : 4, lineCap: .round))
        }
    }

    private var legend: some View {
        let kinds = Array(Set(FlowBinding.boundaryKinds(store))).sorted {
            (BoundaryKind.all.firstIndex(of: $0) ?? 9) < (BoundaryKind.all.firstIndex(of: $1) ?? 9)
        }
        return FloatingBar {
            ForEach(kinds, id: \.self) { kind in
                HStack(spacing: 5) {
                    Circle().fill(BoundaryKind.color(kind)).frame(width: 8, height: 8)
                    Text(BoundaryKind.title(kind)).font(.caption)
                }
                .padding(.horizontal, 4)
            }
        }
    }
}

/// The boundaries, in the inspector: the picked one open for editing, then the whole list.
struct PhysicsInspector: View {
    @Environment(VortiaStore.self) private var store
    @Binding var boundary: Int?

    var body: some View {
        Form {
            let rows = store.flow?["boundaries"].array ?? []
            if let index = boundary, rows.indices.contains(index) {
                Section("\(L10n.t(.segment)) \(index + 1)") {
                    BoundaryFields(index: index)
                }
            }
            Section {
                ForEach(rows.indices, id: \.self) { index in
                    let kind = rows[index]["type"].string ?? "wall"
                    Button {
                        boundary = index
                    } label: {
                        HStack {
                            Circle().fill(BoundaryKind.color(kind)).frame(width: 8, height: 8)
                            Text("\(index + 1) · \(rows[index]["name"].string ?? "")")
                            Spacer()
                            Text(BoundaryKind.title(kind)).foregroundStyle(.secondary)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .listRowBackground(boundary == index ? Color.accentColor.opacity(0.15) : nil)
                }
            } header: {
                Text(L10n.t(.boundaries))
            } footer: {
                Text(L10n.t(.boundaryHint)).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .disabled(store.job.isRunning)
    }
}

/// One contour segment's boundary: its region name, its kind, and the values an open boundary
/// needs. The axis is decided by the contour, not chosen.
private struct BoundaryFields: View {
    @Environment(VortiaStore.self) private var store
    var index: Int

    private var row: JSONValue { store.flow?["boundaries"][index] ?? .null }
    private var kind: String { row["type"].string ?? "wall" }
    private var open: Bool { ["inlet", "outlet", "reservoir"].contains(kind) }

    var body: some View {
        TextField(L10n.t(.regionName), text: field("name"))
        if kind == "axis" {
            Text(L10n.t(.onAxis)).font(.callout).foregroundStyle(.secondary)
        } else {
            Picker(L10n.t(.patchKind), selection: field("type")) {
                ForEach(BoundaryKind.all.filter { $0 != "axis" }, id: \.self) {
                    Text(BoundaryKind.title($0)).tag($0)
                }
            }
        }
        if open {
            NumberField(title: L10n.t(.pressurePa), value: value("pressure"))
            NumberField(title: L10n.t(.temperatureK), value: value("temperature"))
            NumberField(title: L10n.t(.viscosityRatio), value: value("viscosityRatio"))
        }
    }

    private func field(_ key: String) -> Binding<String> {
        Binding(
            get: { store.flow?["boundaries"][index][key].string ?? "" },
            set: { store.flow?["boundaries"][index][key] = .string($0) })
    }

    private func value(_ key: String) -> Binding<Double> {
        Binding(
            get: { store.flow?["boundaries"][index][key].double ?? 0 },
            set: { store.flow?["boundaries"][index][key] = .number($0) })
    }
}

// MARK: Calculation

/// The run controls of a contour project, in the inspector of the calculation stage.
struct CalculationInspector: View {
    @Environment(VortiaStore.self) private var store
    @State private var showAdvanced = false

    var body: some View {
        Form {
            Section(L10n.t(.calculation)) {
                Toggle(L10n.t(.automaticStop), isOn: convergenceEnabled)
                NumberField(title: L10n.t(.iterationLimit), value: FlowBinding.number(store, "solver", "iterations"))
                NumberField(title: L10n.t(.saveEvery), value: FlowBinding.number(store, "solver", "writeInterval"))
                NumberField(title: L10n.t(.acousticCourant), value: FlowBinding.number(store, "solver", "maxCo"))
            }
            Section {
                NumberField(title: L10n.t(.processes), value: FlowBinding.number(store, "solver", "processes"))
            } footer: {
                Text(L10n.t(.processesHint)).foregroundStyle(.secondary)
            }
            Section {
                DisclosureGroup(L10n.t(.advancedCriteria), isExpanded: $showAdvanced) {
                    NumberField(title: L10n.t(.checkEvery), value: FlowBinding.criterion(store, "interval"))
                    NumberField(title: L10n.t(.minimumIterations), value: FlowBinding.criterion(store, "minimumIterations"))
                    NumberField(title: L10n.t(.stableChecks), value: FlowBinding.criterion(store, "stableSamples"))
                    NumberField(title: L10n.t(.massTolerance), value: FlowBinding.criterion(store, "massTolerance"))
                    NumberField(title: L10n.t(.fieldTolerance), value: FlowBinding.criterion(store, "fieldTolerance"))
                    NumberField(title: L10n.t(.equationTolerance), value: FlowBinding.criterion(store, "equationTolerance"))
                }
            }
        }
        .formStyle(.grouped)
        .disabled(store.job.isRunning)
    }

    private var convergenceEnabled: Binding<Bool> {
        Binding(
            get: { store.flow?["solver"]["convergence"]["enabled"].bool ?? true },
            set: { store.flow?["solver"]["convergence"]["enabled"] = .bool($0) })
    }
}

/// How the compressible solver is converging: the monitor's status and the numbers it checks.
struct ConvergenceCard: View {
    var convergence: Convergence

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(L10n.t(.convergence)).font(.headline)
                Spacer()
                Text(convergence.status == "converged" ? L10n.t(.converged) : convergence.status)
                    .font(.callout.weight(.semibold))
                    .foregroundStyle(convergence.status == "converged" ? Color.green : Color.secondary)
            }
            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 4) {
                row(L10n.t(.iteration), convergence.iteration)
                row(L10n.t(.massError), convergence.globalMass)
                row(L10n.t(.localMassError), convergence.localMass)
                row(L10n.t(.fieldChange), convergence.fieldChange)
                row(L10n.t(.updateRate), convergence.equationError)
            }
            ProgressView(
                value: Double(convergence.stableSamples),
                total: Double(max(convergence.requiredSamples, 1))
            ) {
                Text("\(L10n.t(.stableChecks)): \(convergence.stableSamples) / \(convergence.requiredSamples)")
                    .font(.caption)
            }
        }
    }

    private func row(_ title: String, _ value: Double?) -> some View {
        GridRow {
            Text(title).foregroundStyle(.secondary)
            Text(value?.scientific ?? "—").monospacedDigit()
        }
    }
}
