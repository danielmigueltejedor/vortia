import AppKit
import SwiftUI
import VortiaKit
import VortiaShared

@main
struct VortiaApp: App {
    @State private var store = VortiaStore()
    @State private var supervisor = EngineSupervisor()
    @State private var recents = RecentProjects()
    @State private var updater = AppUpdater()

    var body: some Scene {
        // One window: the engine holds one project at a time.
        Window(L10n.t(.appName), id: "main") {
            RootView(
                newProject: newProject, openProject: openProject,
                recentProjects: recents.urls, openRecent: open, appIcon: appIcon
            )
            .environment(store)
            .frame(minWidth: 1040, minHeight: 680)
            .sheet(isPresented: Binding(get: { updater.presented }, set: { updater.presented = $0 })) {
                UpdateSheet(updater: updater)
            }
            .task {
                // The demonstration and its pictures never look for a new version.
                if !Launch.demo { updater.start() }
                if let appearance = Launch.appearance { NSApp.appearance = appearance }
                if Launch.snapshotFolder != nil { NSApp.setActivationPolicy(.regular) }
                if Launch.demo {
                    recents.showDemo()
                    await DemoSession.start(store)
                    if let folder = Launch.snapshotFolder {
                        await Snapshots.take(store, into: folder)
                    } else {
                        await DemoSession.openProject(store)
                    }
                } else {
                    await supervisor.run(store: store)
                }
            }
            .onChange(of: store.project?.root) { _, root in
                if let root, !Launch.demo { recents.add(URL(fileURLWithPath: root)) }
            }
        }
        .defaultSize(width: 1280, height: 820)
        .commands {
            UpdateCommands(updater: updater)
            CommandGroup(replacing: .newItem) {
                Button(L10n.t(.newProject), action: newProject).keyboardShortcut("n")
                Button(L10n.t(.openProject), action: openProject).keyboardShortcut("o")
                Menu(L10n.t(.openRecent)) {
                    ForEach(recents.urls, id: \.self) { url in
                        Button(url.lastPathComponent) { open(url) }
                    }
                    Divider()
                    Button(L10n.t(.clearRecents)) { recents.clear() }
                        .disabled(recents.urls.isEmpty)
                }
            }
            CommandGroup(replacing: .saveItem) {
                Button(L10n.t(.saveProject)) { Task { await store.save() } }
                    .keyboardShortcut("s")
                    .disabled(store.project == nil)
            }
            CommandMenu(L10n.t(.stagesMenu)) {
                ForEach(Array(VortiaStore.Stage.allCases.enumerated()), id: \.element) { index, stage in
                    Button(stage.title) { store.stage = stage }
                        .keyboardShortcut(KeyEquivalent(Character("\(index + 1)")))
                        .disabled(store.project == nil)
                }
            }
            CommandMenu(L10n.t(.simulationMenu)) {
                Button(L10n.t(.runMesh)) { Task { await store.mesh() } }
                    .keyboardShortcut("b")
                    .disabled(!store.canRun)
                Button(L10n.t(.runSolve)) { Task { await store.solve() } }
                    .keyboardShortcut("r")
                    .disabled(!store.canRun)
                Button(L10n.t(.stop)) { Task { await store.cancel() } }
                    .keyboardShortcut(".")
                    .disabled(!store.job.isRunning)
            }
        }

        Settings {
            SettingsView().environment(store)
        }
    }

    /// The app's own icon, when it runs from its bundle.
    private var appIcon: Image? {
        Bundle.main.bundleURL.pathExtension == "app" ? Image(nsImage: NSApp.applicationIconImage) : nil
    }

    /// A project is a folder; the panel names the one to create.
    private func newProject() {
        let panel = NSSavePanel()
        panel.title = L10n.t(.newProject)
        panel.nameFieldStringValue = "Tobera"
        panel.canCreateDirectories = true
        guard panel.runModal() == .OK, let url = panel.url else { return }
        Task { await store.create(at: url) }
    }

    private func openProject() {
        let panel = NSOpenPanel()
        panel.title = L10n.t(.openProject)
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        open(url)
    }

    private func open(_ url: URL) {
        Task { await store.open(url) }
    }
}

/// The projects opened last, newest first, kept between launches.
@MainActor
@Observable
final class RecentProjects {
    private static let key = "vortia.recentProjects"
    private(set) var urls: [URL] = []
    private var demo = false

    init() {
        urls = (UserDefaults.standard.stringArray(forKey: Self.key) ?? [])
            .map { URL(fileURLWithPath: $0) }
            .filter { FileManager.default.fileExists(atPath: $0.path) }
    }

    func add(_ url: URL) {
        guard !demo else { return }
        urls.removeAll { $0.standardizedFileURL == url.standardizedFileURL }
        urls.insert(url, at: 0)
        urls = Array(urls.prefix(8))
        UserDefaults.standard.set(urls.map(\.path), forKey: Self.key)
    }

    func clear() {
        urls = []
        if !demo { UserDefaults.standard.removeObject(forKey: Self.key) }
    }

    /// Example entries for the demonstration, never saved.
    func showDemo() {
        demo = true
        urls = ["Tobera", "Difusor", "Cavidad"].map { URL(fileURLWithPath: "/Users/demo/Vortia/" + $0) }
    }
}

private struct SettingsView: View {
    @Environment(VortiaStore.self) private var store

    var body: some View {
        Form {
            Section(L10n.t(.openFoam)) {
                if let openFoam = store.openFoam, openFoam.found {
                    LabeledContent(L10n.t(.openFoam), value: openFoam.fingerprint ?? "")
                } else {
                    Text(L10n.t(.openFoamMissing)).foregroundStyle(.secondary)
                }
                Button(L10n.t(.detectAgain)) { Task { await store.detect() } }
                    .disabled(store.job.isRunning)
            }
            Section(L10n.t(.edition)) {
                Text(L10n.t(.editionBody)).foregroundStyle(.secondary)
                if !store.engineVersion.isEmpty {
                    LabeledContent("vortia-engine", value: store.engineVersion)
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 480)
        .fixedSize(horizontal: false, vertical: true)
    }
}
