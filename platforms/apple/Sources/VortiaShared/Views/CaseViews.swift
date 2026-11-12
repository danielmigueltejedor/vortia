import SwiftUI
import VortiaKit

/// A binding into the store's setup, for one part of it.
@MainActor
private func setupBinding<T>(_ store: VortiaStore, _ path: WritableKeyPath<CaseSetup, T>) -> Binding<T> {
    Binding(
        get: { store.setup![keyPath: path] },
        set: { store.setup?[keyPath: path] = $0 })
}

// MARK: Geometry

struct GeometryView: View {
    @Environment(VortiaStore.self) private var store

    var body: some View {
        Form {
            Section(L10n.t(.domain)) {
                VectorField(title: L10n.t(.minimum), value: setupBinding(store, \.domain.min))
                VectorField(title: L10n.t(.maximum), value: setupBinding(store, \.domain.max))
                LabeledContent(L10n.t(.size)) {
                    Text(store.setup!.size.map { $0.scientific + " m" }.joined(separator: " × "))
                        .monospacedDigit()
                }
            }
            Section {
                DomainSketch(setup: store.setup!)
                    .frame(height: 260)
                    .listRowInsets(EdgeInsets())
            }
            Section(L10n.t(.compressibleTitle)) {
                Text(L10n.t(.compressibleBody)).font(.callout).foregroundStyle(.secondary)
                Button(L10n.t(.makeNozzle)) { Task { await store.makeNozzle(NozzleDimensions()) } }
                    .disabled(store.job.isRunning)
            }
        }
        .formStyle(.grouped)
    }
}

/// The box in the x–y plane with its patches named on their sides: what the numbers describe.
struct DomainSketch: View {
    var setup: CaseSetup

    var body: some View {
        Canvas { context, size in
            let w = max(setup.size[0], 1e-12)
            let h = max(setup.size[1], 1e-12)
            let scale = min((size.width - 120) / w, (size.height - 60) / h)
            let rect = CGRect(
                x: (size.width - w * scale) / 2, y: (size.height - h * scale) / 2,
                width: w * scale, height: h * scale)
            let box = Path(roundedRect: rect, cornerRadius: 4)
            context.fill(box, with: .color(.accentColor.opacity(0.12)))
            context.stroke(box, with: .color(.accentColor), lineWidth: 2)
            let names = Dictionary(
                setup.mesh.patches.map { ($0.face, $0.name) }, uniquingKeysWith: { a, _ in a })
            let labels: [(Int, CGPoint)] = [
                (0, CGPoint(x: rect.minX - 8, y: rect.midY)), (1, CGPoint(x: rect.maxX + 8, y: rect.midY)),
                (2, CGPoint(x: rect.midX, y: rect.maxY + 14)), (3, CGPoint(x: rect.midX, y: rect.minY - 14)),
            ]
            for (face, point) in labels {
                let anchor: UnitPoint = face == 0 ? .trailing : face == 1 ? .leading : .center
                context.draw(
                    Text(names[face] ?? "").font(.caption).foregroundStyle(.secondary),
                    at: point, anchor: anchor)
            }
        }
        .accessibilityHidden(true)
    }
}

// MARK: Mesh

struct MeshView: View {
    @Environment(VortiaStore.self) private var store
    private let kinds = ["wall", "patch", "empty", "symmetry"]

    var body: some View {
        Form {
            Section(L10n.t(.cells)) {
                ForEach(0..<3, id: \.self) { axis in
                    Stepper(
                        value: Binding(
                            get: { store.setup!.mesh.cells[safe: axis] },
                            set: { store.setup?.mesh.cells[axis] = max(1, $0) }),
                        in: 1...2000
                    ) {
                        LabeledContent(["x", "y", "z"][axis]) {
                            Text("\(store.setup!.mesh.cells[safe: axis])").monospacedDigit()
                        }
                    }
                }
                LabeledContent(L10n.t(.totalCells)) {
                    Text(store.setup!.mesh.cells.reduce(1, *).formatted()).monospacedDigit()
                }
            }
            Section(L10n.t(.grading)) {
                VectorField(title: L10n.t(.grading), value: setupBinding(store, \.mesh.grading))
            }
            Section(L10n.t(.patches)) {
                ForEach(store.setup!.mesh.patches.indices, id: \.self) { index in
                    HStack {
                        TextField(
                            "", text: Binding(
                                get: { store.setup!.mesh.patches[index].name },
                                set: { store.setup?.mesh.patches[index].name = $0 }))
                        Picker(L10n.t(.patchKind), selection: Binding(
                            get: { store.setup!.mesh.patches[index].kind },
                            set: { store.setup?.mesh.patches[index].kind = $0 })
                        ) {
                            ForEach(kinds, id: \.self) { Text($0).tag($0) }
                        }
                        .labelsHidden()
                        .frame(width: 130)
                    }
                }
            }
        }
        .formStyle(.grouped)
    }
}

// MARK: Physics

struct PhysicsView: View {
    @Environment(VortiaStore.self) private var store
    private let velocityKinds = ["noSlip", "fixedValue", "zeroGradient", "symmetry", "empty"]
    private let pressureKinds = ["zeroGradient", "fixedValue", "symmetry", "empty"]

    var body: some View {
        Form {
            Section(L10n.t(.fluid)) {
                TextField(L10n.t(.fluid), text: setupBinding(store, \.material.name))
                NumberField(title: L10n.t(.viscosity), value: setupBinding(store, \.material.nu))
            }
            Section(L10n.t(.solver)) {
                Picker(L10n.t(.solver), selection: setupBinding(store, \.solver)) {
                    Text(L10n.t(.transient)).tag("icoFoam")
                    Text(L10n.t(.steady)).tag("simpleFoam")
                }
                Picker(L10n.t(.turbulence), selection: setupBinding(store, \.turbulence.model)) {
                    Text(L10n.t(.laminar)).tag("laminar")
                    Text(L10n.t(.kOmegaSST)).tag("kOmegaSST")
                }
                if store.setup!.turbulence.model == "kOmegaSST" {
                    NumberField(title: L10n.t(.intensity), value: setupBinding(store, \.turbulence.intensity))
                    NumberField(title: L10n.t(.lengthScale), value: setupBinding(store, \.turbulence.lengthScale))
                }
            }
            Section(L10n.t(.boundaries)) {
                ForEach(store.setup!.boundaries.indices, id: \.self) { index in
                    BoundaryRow(index: index, velocityKinds: velocityKinds, pressureKinds: pressureKinds)
                }
            }
            Section(L10n.t(.initialField)) {
                VectorField(title: L10n.t(.velocity), value: setupBinding(store, \.internalField.velocity))
                NumberField(title: L10n.t(.pressure), value: setupBinding(store, \.internalField.pressure))
            }
        }
        .formStyle(.grouped)
    }
}

private struct BoundaryRow: View {
    @Environment(VortiaStore.self) private var store
    var index: Int
    var velocityKinds: [String]
    var pressureKinds: [String]

    private func bind<T>(_ path: WritableKeyPath<CaseSetup.Boundary, T>) -> Binding<T> {
        Binding(
            get: { store.setup!.boundaries[index][keyPath: path] },
            set: { store.setup?.boundaries[index][keyPath: path] = $0 })
    }

    var body: some View {
        DisclosureGroup {
            Picker(L10n.t(.velocity), selection: bind(\.velocity)) {
                ForEach(velocityKinds, id: \.self) { Text($0).tag($0) }
            }
            if store.setup!.boundaries[index].velocity == "fixedValue" {
                VectorField(title: "U (m/s)", value: bind(\.velocityValue))
            }
            Picker(L10n.t(.pressure), selection: bind(\.pressure)) {
                ForEach(pressureKinds, id: \.self) { Text($0).tag($0) }
            }
            if store.setup!.boundaries[index].pressure == "fixedValue" {
                NumberField(title: "p (m²/s²)", value: bind(\.pressureValue))
            }
        } label: {
            HStack {
                Text(store.setup!.boundaries[index].patch).font(.body.weight(.medium))
                Spacer()
                Text(store.setup!.boundaries[index].velocity).foregroundStyle(.secondary)
            }
        }
    }
}

// MARK: Time

struct TimeView: View {
    @Environment(VortiaStore.self) private var store

    var body: some View {
        Form {
            Section(L10n.t(.time)) {
                NumberField(title: L10n.t(.startTime), value: setupBinding(store, \.time.startTime))
                NumberField(title: L10n.t(.endTime), value: setupBinding(store, \.time.endTime))
                NumberField(title: L10n.t(.timeStep), value: setupBinding(store, \.time.deltaT))
                Stepper(value: setupBinding(store, \.time.writeInterval), in: 1...100_000) {
                    LabeledContent(L10n.t(.writeInterval)) {
                        Text("\(store.setup!.time.writeInterval)").monospacedDigit()
                    }
                }
                Toggle(L10n.t(.adjustTimeStep), isOn: setupBinding(store, \.time.adjustTimeStep))
                if store.setup!.time.adjustTimeStep {
                    NumberField(title: L10n.t(.maxCourant), value: setupBinding(store, \.time.maxCo))
                }
            }
            Section(L10n.t(.numerics)) {
                Picker(L10n.t(.timeScheme), selection: setupBinding(store, \.numerics.ddt)) {
                    Text("Euler").tag("euler")
                    Text("Backward").tag("backward")
                    Text("Steady state").tag("steadyState")
                }
                Stepper(value: setupBinding(store, \.numerics.pisoCorrectors), in: 1...10) {
                    LabeledContent(L10n.t(.correctors)) {
                        Text("\(store.setup!.numerics.pisoCorrectors)").monospacedDigit()
                    }
                }
                NumberField(title: L10n.t(.toleranceP), value: setupBinding(store, \.numerics.toleranceP))
                NumberField(title: L10n.t(.toleranceU), value: setupBinding(store, \.numerics.toleranceU))
            }
        }
        .formStyle(.grouped)
    }
}

// MARK: Checks

/// The inspector: what the engine thinks of the case, and whether OpenFOAM is there.
struct ChecksView: View {
    @Environment(VortiaStore.self) private var store

    var body: some View {
        Form {
            Section(L10n.t(.checks)) {
                if let validation = store.validation {
                    if validation.issues.isEmpty {
                        Label(L10n.t(.noIssues), systemImage: "checkmark.circle")
                            .foregroundStyle(.green)
                    }
                    ForEach(validation.issues, id: \.self) { issue in
                        Label(issue.message, systemImage: issue.isError ? "xmark.octagon" : "exclamationmark.triangle")
                            .foregroundStyle(issue.isError ? .red : .orange)
                    }
                    if let reynolds = validation.reynolds {
                        LabeledContent(L10n.t(.reynolds), value: reynolds.scientific)
                    }
                    if let courant = validation.courant, courant > 0 {
                        LabeledContent(L10n.t(.courant), value: courant.scientific)
                    }
                } else {
                    ProgressView()
                }
            }
            Section(L10n.t(.openFoam)) {
                if let openFoam = store.openFoam {
                    if openFoam.found {
                        Label(openFoam.fingerprint ?? "", systemImage: "checkmark.seal")
                            .font(.callout)
                    } else {
                        Text(L10n.t(.openFoamMissing)).font(.callout).foregroundStyle(.secondary)
                        Button(L10n.t(.detectAgain)) { Task { await store.detect() } }
                    }
                }
            }
        }
        .formStyle(.grouped)
    }
}
