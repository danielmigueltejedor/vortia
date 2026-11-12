import SwiftUI
import VortiaKit

/// The compressible setup: mesh sizes, the gas and its model, a boundary per contour segment,
/// the initial state and when to stop. The same fields, limits and defaults as the desktop app's
/// flow panel; the engine normalises what is saved.
struct FlowSetupView: View {
    @Environment(VortiaStore.self) private var store
    @State private var showAdvanced = false

    var body: some View {
        if store.flow == nil {
            EmptyState(symbol: "scribble.variable", title: L10n.t(.noContour), message: L10n.t(.noContourBody))
        } else {
            Form {
                Section(L10n.t(.mesh)) {
                    number(L10n.t(.globalSize), "mesh", "sizeMm")
                    number(L10n.t(.wallSize), "mesh", "wallSizeMm")
                    number(L10n.t(.wedgeAngle), "mesh", "wedgeAngle")
                }
                Section(L10n.t(.gasModel)) {
                    Picker(L10n.t(.flowModel), selection: text("material", "turbulence", fallback: "SpalartAllmaras")) {
                        Text("Spalart–Allmaras").tag("SpalartAllmaras")
                        Text(L10n.t(.laminar)).tag("laminar")
                        Text(L10n.t(.inviscid)).tag("inviscid")
                    }
                    number(L10n.t(.molecularWeight), "material", "molecularWeight")
                    number(L10n.t(.specificHeat), "material", "Cp")
                    number(L10n.t(.dynamicViscosity), "material", "mu")
                    number(L10n.t(.conductivity), "material", "conductivity")
                }
                Section(L10n.t(.boundaries)) {
                    Text(L10n.t(.boundaryHint)).font(.caption).foregroundStyle(.secondary)
                    ForEach(store.flow?["boundaries"].array.indices ?? 0..<0, id: \.self) { index in
                        BoundaryEditor(index: index)
                    }
                }
                Section(L10n.t(.initialField)) {
                    number(L10n.t(.initialPressure), "initial", "pressure")
                    number(L10n.t(.initialTemperature), "initial", "temperature")
                }
                Section(L10n.t(.calculation)) {
                    Toggle(L10n.t(.automaticStop), isOn: convergenceEnabled)
                    number(L10n.t(.iterationLimit), "solver", "iterations")
                    number(L10n.t(.saveEvery), "solver", "writeInterval")
                    number(L10n.t(.acousticCourant), "solver", "maxCo")
                    number(L10n.t(.processes), "solver", "processes")
                    Text(L10n.t(.processesHint)).font(.caption).foregroundStyle(.secondary)
                    DisclosureGroup(L10n.t(.advancedCriteria), isExpanded: $showAdvanced) {
                        criterion(L10n.t(.checkEvery), "interval")
                        criterion(L10n.t(.minimumIterations), "minimumIterations")
                        criterion(L10n.t(.stableChecks), "stableSamples")
                        criterion(L10n.t(.massTolerance), "massTolerance")
                        criterion(L10n.t(.fieldTolerance), "fieldTolerance")
                        criterion(L10n.t(.equationTolerance), "equationTolerance")
                    }
                }
            }
            .formStyle(.grouped)
            .disabled(store.job.isRunning)
        }
    }

    private func number(_ title: String, _ section: String, _ key: String) -> some View {
        NumberField(
            title: title,
            value: Binding(
                get: { store.flow?[section][key].double ?? 0 },
                set: { store.flow?[section][key] = .number($0) }))
    }

    private func criterion(_ title: String, _ key: String) -> some View {
        NumberField(
            title: title,
            value: Binding(
                get: { store.flow?["solver"]["convergence"][key].double ?? 0 },
                set: { store.flow?["solver"]["convergence"][key] = .number($0) }))
    }

    private func text(_ section: String, _ key: String, fallback: String) -> Binding<String> {
        Binding(
            get: { store.flow?[section][key].string ?? fallback },
            set: { store.flow?[section][key] = .string($0) })
    }

    private var convergenceEnabled: Binding<Bool> {
        Binding(
            get: { store.flow?["solver"]["convergence"]["enabled"].bool ?? true },
            set: { store.flow?["solver"]["convergence"]["enabled"] = .bool($0) })
    }
}

/// One contour segment's boundary: its region name, its kind, and the values an open boundary
/// needs. The axis is decided by the contour, not chosen.
private struct BoundaryEditor: View {
    @Environment(VortiaStore.self) private var store
    var index: Int

    private var row: JSONValue { store.flow?["boundaries"][index] ?? .null }
    private var kind: String { row["type"].string ?? "wall" }
    private var open: Bool { ["inlet", "outlet", "reservoir"].contains(kind) }

    var body: some View {
        DisclosureGroup {
            TextField(L10n.t(.regionName), text: field("name"))
            if kind != "axis" {
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
        } label: {
            HStack {
                Circle().fill(BoundaryKind.color(kind)).frame(width: 8, height: 8)
                Text("\(index + 1) · \(row["name"].string ?? "")").font(.body.weight(.medium))
                Spacer()
                Text(BoundaryKind.title(kind)).foregroundStyle(.secondary)
            }
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

/// How the compressible solver is converging: the monitor's status and the numbers it checks.
struct ConvergenceCard: View {
    var convergence: Convergence

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(L10n.t(.convergence)).font(.headline)
                Spacer()
                Text(convergence.status).font(.callout.weight(.semibold))
                    .foregroundStyle(convergence.status == "converged" ? .green : .secondary)
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
