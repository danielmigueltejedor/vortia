import Foundation
import VortiaKit
import VortiaShared

/// Keeps the app joined to vortia-engine: started with the app, started again with a growing
/// pause if it goes, and asked to stop when the app quits. A dead engine must never look like a
/// calm, idle case.
@MainActor
final class EngineSupervisor {
    private var connection: EngineConnection?

    /// The engine inside the app, or one named by `VORTIA_ENGINE_PATH` for a build run from
    /// the tree.
    private var executable: URL? {
        if let path = ProcessInfo.processInfo.environment["VORTIA_ENGINE_PATH"] {
            return URL(fileURLWithPath: path)
        }
        let bundled = Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/vortia-engine")
        return FileManager.default.isExecutableFile(atPath: bundled.path) ? bundled : nil
    }

    func run(store: VortiaStore) async {
        guard let executable else {
            store.detach(reason: "vortia-engine is not inside the app (set VORTIA_ENGINE_PATH)")
            return
        }
        var pause = 1.0
        while !Task.isCancelled {
            store.connectionAttempt()
            do {
                let connection = try EngineConnection(executable: executable)
                self.connection = connection
                await store.attach(connection)
                pause = 1
                for await event in connection.events {
                    store.apply(event)
                }
                store.detach(reason: L10n.t(.engineLost))
            } catch {
                store.detach(reason: error.localizedDescription)
            }
            connection = nil
            try? await Task.sleep(for: .seconds(pause))
            pause = min(pause * 2, 15)
        }
    }

    func stop() {
        connection?.stop()
    }
}
