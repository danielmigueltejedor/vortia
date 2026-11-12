import AppKit
import SwiftUI
import VortiaKit
import VortiaShared

@main
struct VortiaApp: App {
    @State private var store = VortiaStore()
    @State private var supervisor = EngineSupervisor()

    var body: some Scene {
        // One window: the engine holds one project at a time.
        Window(L10n.t(.appName), id: "main") {
            RootView(newProject: newProject, openProject: openProject)
                .environment(store)
                .frame(minWidth: 980, minHeight: 640)
                .task { await supervisor.run(store: store) }
        }
        .defaultSize(width: 1280, height: 820)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button(L10n.t(.newProject), action: newProject).keyboardShortcut("n")
                Button(L10n.t(.openProject), action: openProject).keyboardShortcut("o")
            }
            CommandGroup(replacing: .saveItem) {
                Button(L10n.t(.saveProject)) { Task { await store.save() } }
                    .keyboardShortcut("s")
                    .disabled(store.project == nil)
            }
            CommandMenu(L10n.t(.run)) {
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

    /// A project is a folder; the panel names the one to create.
    private func newProject() {
        let panel = NSSavePanel()
        panel.title = L10n.t(.newProject)
        panel.nameFieldStringValue = "Cavity"
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
        Task { await store.open(url) }
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
