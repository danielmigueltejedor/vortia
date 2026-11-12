import SwiftUI
import VortiaKit

/// The window: a welcome with no project, else the case's stages in a sidebar, the stage in the
/// middle and its checks in the inspector. The system draws the chrome; the toolbar carries the
/// two actions a case is for, mesh and solve.
public struct RootView: View {
    @Environment(VortiaStore.self) private var store
    @SceneStorage("vortia.inspector") private var inspectorOpen = true
    var newProject: () -> Void
    var openProject: () -> Void

    public init(newProject: @escaping () -> Void, openProject: @escaping () -> Void) {
        self.newProject = newProject
        self.openProject = openProject
    }

    public var body: some View {
        Group {
            if store.project == nil {
                Welcome(newProject: newProject, openProject: openProject)
            } else {
                workspace
            }
        }
        .overlay(alignment: .top) { Banner() }
    }

    private var workspace: some View {
        @Bindable var store = store
        return NavigationSplitView {
            List(selection: Binding(get: { store.stage }, set: { if let s = $0 { store.stage = s } })) {
                Section(store.project?.name ?? "") {
                    ForEach([VortiaStore.Stage.geometry, .mesh, .physics, .time]) { stage in
                        Label(stage.title, systemImage: stage.symbol).tag(stage)
                    }
                }
                Section {
                    ForEach([VortiaStore.Stage.run, .results, .engineering]) { stage in
                        Label(stage.title, systemImage: stage.symbol).tag(stage)
                    }
                }
            }
            .navigationSplitViewColumnWidth(min: 190, ideal: 220, max: 280)
        } detail: {
            StageView(stage: store.stage)
                .navigationTitle(store.stage.title)
                .navigationSubtitle(store.project?.name ?? "")
        }
        .inspector(isPresented: $inspectorOpen) {
            ChecksView()
                .inspectorColumnWidth(min: 240, ideal: 280, max: 360)
        }
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                RunControls()
                Button {
                    inspectorOpen.toggle()
                } label: {
                    Label(L10n.t(.checks), systemImage: "checklist")
                }
                .help(L10n.t(.checks))
            }
        }
    }
}

extension VortiaStore.Stage {
    var title: String {
        switch self {
        case .geometry: L10n.t(.geometry)
        case .mesh: L10n.t(.mesh)
        case .physics: L10n.t(.physics)
        case .time: L10n.t(.time)
        case .run: L10n.t(.run)
        case .results: L10n.t(.results)
        case .engineering: L10n.t(.engineering)
        }
    }

    var symbol: String {
        switch self {
        case .geometry: "cube"
        case .mesh: "square.grid.3x3"
        case .physics: "wind"
        case .time: "clock"
        case .run: "play.circle"
        case .results: "chart.xyaxis.line"
        case .engineering: "ruler"
        }
    }
}

private struct StageView: View {
    @Environment(VortiaStore.self) private var store
    var stage: VortiaStore.Stage

    var body: some View {
        if store.setup == nil {
            ProgressView()
        } else {
            switch stage {
            case .geometry: GeometryView()
            case .mesh: MeshView()
            case .physics: PhysicsView()
            case .time: TimeView()
            case .run: RunView()
            case .results: ResultsView()
            case .engineering: EngineeringView()
            }
        }
    }
}

/// Mesh and Solve, or Stop while either runs.
struct RunControls: View {
    @Environment(VortiaStore.self) private var store

    var body: some View {
        if store.job.isRunning {
            Button(role: .cancel) {
                Task { await store.cancel() }
            } label: {
                Label(L10n.t(.stop), systemImage: "stop.fill")
            }
            .help(L10n.t(.stop))
        } else {
            Button {
                Task { await store.mesh() }
            } label: {
                Label(L10n.t(.runMesh), systemImage: "square.grid.3x3")
            }
            .disabled(!store.canRun)
            .help(L10n.t(.runMesh))
            Button {
                Task { await store.solve() }
            } label: {
                Label(L10n.t(.runSolve), systemImage: "play.fill")
            }
            .disabled(!store.canRun)
            .help(L10n.t(.runSolve))
        }
    }
}

private struct Welcome: View {
    var newProject: () -> Void
    var openProject: () -> Void

    var body: some View {
        EmptyState(
            symbol: "wind", title: L10n.t(.welcomeTitle), message: L10n.t(.welcomeBody)
        ) {
            HStack(spacing: 12) {
                Button(L10n.t(.newProject), action: newProject)
                    .buttonStyle(.borderedProminent)
                Button(L10n.t(.openProject), action: openProject)
                    .buttonStyle(.bordered)
            }
            .controlSize(.large)
            .padding(.top, 6)
        }
    }
}

/// Said once and out of the way: the engine is starting or was lost, or a request failed.
private struct Banner: View {
    @Environment(VortiaStore.self) private var store

    var body: some View {
        VStack(spacing: 8) {
            switch store.connection {
            case .starting:
                row(symbol: "hourglass", text: L10n.t(.engineStarting), dismiss: nil)
            case .lost(let reason):
                row(symbol: "bolt.horizontal.circle", text: L10n.t(.engineLost) + ": " + reason, dismiss: nil)
            case .ready:
                EmptyView()
            }
            if let error = store.error {
                row(symbol: "exclamationmark.triangle", text: error) { store.dismissError() }
            }
        }
        .padding(12)
        .frame(maxWidth: 560)
    }

    private func row(symbol: String, text: String, dismiss: (() -> Void)?) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: symbol).foregroundStyle(.secondary)
            Text(text).font(.callout)
            Spacer(minLength: 0)
            if let dismiss {
                Button(L10n.t(.dismiss), systemImage: "xmark", action: dismiss)
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
            }
        }
        .card()
    }
}
