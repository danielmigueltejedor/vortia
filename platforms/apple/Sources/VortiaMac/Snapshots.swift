import AppKit
import VortiaKit
import VortiaShared

/// How the app was started: normally, or with the demonstration engine, and optionally to
/// photograph every stage and quit (`--snapshot <folder> [--appearance light|dark]`).
enum Launch {
    private static let arguments = ProcessInfo.processInfo.arguments

    private static func value(after flag: String) -> String? {
        guard let index = arguments.firstIndex(of: flag), arguments.indices.contains(index + 1) else {
            return nil
        }
        return arguments[index + 1]
    }

    static var snapshotFolder: URL? {
        value(after: "--snapshot").map { URL(fileURLWithPath: $0, isDirectory: true) }
    }

    static var demo: Bool { arguments.contains("--demo") || snapshotFolder != nil }

    static var appearance: NSAppearance? {
        switch value(after: "--appearance") {
        case "dark": NSAppearance(named: .darkAqua)
        case "light": NSAppearance(named: .aqua)
        default: nil
        }
    }
}

/// The demonstration engine: attached with no project, so the welcome shows, then the solved
/// nozzle opened with its run history.
@MainActor
enum DemoSession {
    static let project = URL(fileURLWithPath: "/Users/demo/Vortia/Tobera", isDirectory: true)

    static func start(_ store: VortiaStore) async {
        await store.attach(DemoEngine())
    }

    static func openProject(_ store: VortiaStore) async {
        await store.open(project)
        try? await Task.sleep(for: .milliseconds(400))
        for event in DemoEngine.history() { store.apply(event) }
        try? await Task.sleep(for: .milliseconds(600))
    }
}

/// Photographs the window at every stage the sidebar lists, then quits.
@MainActor
enum Snapshots {
    static func take(_ store: VortiaStore, into folder: URL) async {
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try? await Task.sleep(for: .seconds(1))
        guard let window = NSApp.windows.first(where: { $0.isVisible && $0.frame.width > 600 }) else {
            FileHandle.standardError.write(Data("snapshots: no window\n".utf8))
            NSApp.terminate(nil)
            return
        }
        window.setContentSize(NSSize(width: 1280, height: 820))
        window.center()
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
        try? await Task.sleep(for: .seconds(1.5))
        save(window, to: folder.appendingPathComponent("00-welcome.png"))
        await DemoSession.openProject(store)
        for (index, stage) in store.visibleStages.enumerated() {
            store.stage = stage
            try? await Task.sleep(for: .seconds(2.5))
            let name = String(format: "%02d-%@", index + 1, stage.rawValue)
            save(window, to: folder.appendingPathComponent(name + ".png"))
        }
        NSApp.terminate(nil)
    }

    /// The window as the screen shows it, with its glass; drawn from the view tree when the
    /// screen cannot be read.
    private static func save(_ window: NSWindow, to url: URL) {
        let capture = Process()
        capture.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        capture.arguments = ["-x", "-o", "-l\(window.windowNumber)", url.path]
        try? capture.run()
        capture.waitUntilExit()
        let size = (try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? Int) ?? 0
        if capture.terminationStatus == 0, size > 10_000 { return }

        guard let view = window.contentView?.superview ?? window.contentView,
            let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds)
        else { return }
        view.cacheDisplay(in: view.bounds, to: bitmap)
        try? bitmap.representation(using: .png, properties: [:])?.write(to: url)
        FileHandle.standardError.write(Data("snapshots: \(url.lastPathComponent) drawn from the views\n".utf8))
    }
}
