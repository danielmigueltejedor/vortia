import SwiftUI
import VortiaKit

/// The window: a welcome with no project, else Vortia's six stages in the sidebar, the stage's
/// work in the middle and its settings in the inspector. The system draws the chrome; the
/// toolbar carries the two actions a case is for, mesh and solve.
public struct RootView: View {
    @Environment(VortiaStore.self) private var store
    @SceneStorage("vortia.inspector") private var inspectorOpen = true
    @State private var sketchSelection: SketchSelection?
    @State private var boundary: Int?
    var newProject: () -> Void
    var openProject: () -> Void
    var recentProjects: [URL]
    var openRecent: (URL) -> Void
    var appIcon: Image?

    public init(
        newProject: @escaping () -> Void, openProject: @escaping () -> Void,
        recentProjects: [URL] = [], openRecent: @escaping (URL) -> Void = { _ in },
        appIcon: Image? = nil
    ) {
        self.newProject = newProject
        self.openProject = openProject
        self.recentProjects = recentProjects
        self.openRecent = openRecent
        self.appIcon = appIcon
    }

    public var body: some View {
        Group {
            if store.project == nil {
                Welcome(
                    newProject: newProject, openProject: openProject, recentProjects: recentProjects,
                    openRecent: openRecent, appIcon: appIcon)
            } else {
                workspace
            }
        }
        .overlay(alignment: .bottom) { Banner() }
        .onChange(of: store.sketch?.points.count) {
            sketchSelection = nil
            boundary = nil
        }
    }

    private var workspace: some View {
        NavigationSplitView {
            List(selection: Binding(get: { store.stage }, set: { if let s = $0 { store.stage = s } })) {
                Section(store.project?.name ?? "") {
                    ForEach(store.visibleStages) { stage in
                        StageRow(stage: stage).tag(stage)
                    }
                }
            }
            .navigationSplitViewColumnWidth(min: 190, ideal: 210, max: 260)
        } detail: {
            StageView(stage: store.stage, sketchSelection: $sketchSelection, boundary: $boundary)
                .navigationTitle(store.stage.title)
                .navigationSubtitle(store.project?.name ?? "")
        }
        .inspector(isPresented: $inspectorOpen) {
            StageInspector(stage: store.stage, sketchSelection: $sketchSelection, boundary: $boundary)
                .inspectorColumnWidth(min: 260, ideal: 300, max: 400)
        }
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                RunControls()
                Button {
                    inspectorOpen.toggle()
                } label: {
                    Label(L10n.t(.inspector), systemImage: "sidebar.right")
                }
                .help(L10n.t(.inspector))
            }
        }
    }
}

extension VortiaStore.Stage {
    public var title: String {
        switch self {
        case .project: L10n.t(.project)
        case .geometry: L10n.t(.geometry)
        case .mesh: L10n.t(.mesh)
        case .physics: L10n.t(.physics)
        case .calculation: L10n.t(.run)
        case .results: L10n.t(.results)
        }
    }

    /// The same pictures as Vortia's sidebar: a folder, a pencil, a grid, layers, a chip and a
    /// chart.
    var symbol: String {
        switch self {
        case .project: "folder"
        case .geometry: "pencil.and.outline"
        case .mesh: "squareshape.split.3x3"
        case .physics: "square.3.layers.3d"
        case .calculation: "cpu"
        case .results: "chart.xyaxis.line"
        }
    }

    /// What to do in this stage, said as the next step.
    var guidance: String {
        switch self {
        case .project: L10n.t(.guideProject)
        case .geometry: L10n.t(.guideGeometry)
        case .mesh: L10n.t(.guideMesh)
        case .physics: L10n.t(.guidePhysics)
        case .calculation: L10n.t(.guideCalculation)
        case .results: L10n.t(.guideResults)
        }
    }
}

/// A stage in the sidebar, with a mark when it has something to say: a run in progress, or
/// results ready.
private struct StageRow: View {
    @Environment(VortiaStore.self) private var store
    var stage: VortiaStore.Stage

    var body: some View {
        HStack {
            Label(stage.title, systemImage: stage.symbol)
            Spacer()
            switch stage {
            case .calculation where store.job.isRunning:
                ProgressView().controlSize(.mini)
            case .results where store.results != nil:
                Circle().fill(Color.accentColor).frame(width: 6, height: 6)
            default:
                EmptyView()
            }
        }
    }
}

private struct StageView: View {
    @Environment(VortiaStore.self) private var store
    var stage: VortiaStore.Stage
    @Binding var sketchSelection: SketchSelection?
    @Binding var boundary: Int?

    var body: some View {
        if store.setup == nil {
            ProgressView()
        } else {
            switch stage {
            case .project: ProjectOverview()
            case .geometry:
                if store.isFlowProject { GeometryEditor(selection: $sketchSelection) } else { GeometryView() }
            case .mesh:
                if store.isFlowProject { MeshStage() } else { MeshView() }
            case .physics:
                if store.isFlowProject { PhysicsStage(boundary: $boundary) } else { PhysicsView() }
            case .calculation: CalculationMonitor()
            case .results: ResultsViewer()
            }
        }
    }
}

/// The inspector follows the stage: what is selected and the stage's settings.
private struct StageInspector: View {
    @Environment(VortiaStore.self) private var store
    var stage: VortiaStore.Stage
    @Binding var sketchSelection: SketchSelection?
    @Binding var boundary: Int?

    var body: some View {
        if store.setup == nil {
            ProgressView()
        } else {
            switch stage {
            case .geometry where store.isFlowProject: GeometryInspector(selection: $sketchSelection)
            case .mesh where store.isFlowProject: MeshInspector()
            case .physics where store.isFlowProject: PhysicsInspector(boundary: $boundary)
            case .calculation: if store.isFlowProject { CalculationInspector() } else { TimeView() }
            case .results: ResultsInspector()
            default: ChecksView()
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
                Label(L10n.t(.runMesh), systemImage: "squareshape.split.3x3")
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
    var recentProjects: [URL]
    var openRecent: (URL) -> Void
    var appIcon: Image?

    var body: some View {
        HStack(spacing: 0) {
            VStack(spacing: 16) {
                Spacer()
                if let appIcon {
                    appIcon.resizable().interpolation(.high).frame(width: 112, height: 112)
                } else {
                    Image(systemName: "wind").font(.system(size: 64, weight: .light)).foregroundStyle(.tint)
                }
                Text(L10n.t(.appName)).font(.system(size: 40, weight: .bold))
                Text(L10n.t(.welcomeTitle)).font(.title3).foregroundStyle(.secondary)
                Text(L10n.t(.welcomeBody))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 420)
                HStack(spacing: 12) {
                    Button(action: newProject) {
                        Label(L10n.t(.newProject), systemImage: "plus")
                    }
                    .buttonStyle(.borderedProminent)
                    Button(action: openProject) {
                        Label(L10n.t(.openProject), systemImage: "folder")
                    }
                    .buttonStyle(.bordered)
                }
                .controlSize(.large)
                .padding(.top, 8)
                Spacer()
            }
            .padding(40)
            .frame(maxWidth: .infinity)
            if !recentProjects.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text(L10n.t(.recentProjects))
                        .font(.headline)
                        .padding(.horizontal, 12)
                        .padding(.bottom, 6)
                    ForEach(recentProjects, id: \.self) { url in
                        Button {
                            openRecent(url)
                        } label: {
                            HStack(spacing: 10) {
                                Image(systemName: "folder.fill").foregroundStyle(.tint)
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(url.lastPathComponent).font(.body.weight(.medium))
                                    Text(url.deletingLastPathComponent().path)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                        .truncationMode(.middle)
                                }
                                Spacer(minLength: 0)
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 7)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                    Spacer()
                }
                .padding(.vertical, 40)
                .padding(.horizontal, 16)
                .frame(width: 320)
                .frame(maxHeight: .infinity)
                .background(.background.secondary)
            }
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
        .padding(16)
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
