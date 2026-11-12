import Foundation
import Testing
import VortiaKit

@testable import VortiaShared

/// An engine that answers from a script and remembers what it was asked.
private final class ScriptedEngine: EngineClient, @unchecked Sendable {
    var answers: [String: String]
    private(set) var asked: [String] = []

    init(_ answers: [String: String]) { self.answers = answers }

    func send(_ request: EngineRequest) async throws -> Data {
        asked.append(request.command)
        guard let answer = answers[request.command] else { throw EngineError("no \(request.command)") }
        return Data(answer.utf8)
    }
}

@MainActor
@Suite("The store")
struct StoreTests {
    @Test("progress becomes residual points, one per field with a positive residual")
    func residuals() {
        let store = VortiaStore()
        store.apply(.progress(VortiaKit.Progress(
            time: 0.01, courantMean: 0.1, courantMax: 0.5,
            residuals: [
                Residual(field: "Ux", initial: 0.5, final: 1e-6, iterations: 3),
                Residual(field: "p", initial: 0, final: 0, iterations: 0),
            ])))
        #expect(store.residuals.map(\.field) == ["Ux"])
        #expect(store.latest?.courantMax == 0.5)
    }

    @Test("a job's events move the state, and a failure keeps its reason")
    func job() {
        let store = VortiaStore()
        store.apply(.job(JobEvent(state: "running", phase: "mesh")))
        #expect(store.job == .running(phase: "mesh"))
        store.apply(.job(JobEvent(state: "finished", phase: "mesh", ok: false, summary: "blockMesh failed")))
        #expect(store.job == .finished(phase: "mesh", ok: false, summary: "blockMesh failed"))
    }

    @Test("the log keeps its tail")
    func logTail() {
        let store = VortiaStore()
        for _ in 0..<2000 { store.apply(.log(String(repeating: "x", count: 99) + "\n")) }
        #expect(store.log.count <= 120_000)
        #expect(store.log.hasSuffix("x\n"))
    }

    @Test("an engine that is lost says so and ends a run")
    func lost() {
        let store = VortiaStore()
        store.apply(.job(JobEvent(state: "running", phase: "solve")))
        store.detach(reason: "gone")
        #expect(store.connection == .lost("gone"))
        #expect(store.job == .finished(phase: "solve", ok: false, summary: "gone"))
    }

    @Test("attaching reads OpenFOAM's status and the engine's version")
    func attach() async {
        let engine = ScriptedEngine([
            "hello": #"{"protocol":1,"version":"0.6.0","openfoam":{"found":false,"error":"none"},"project":null,"running":false}"#
        ])
        let store = VortiaStore()
        await store.attach(engine)
        #expect(store.connection == .ready)
        #expect(store.engineVersion == "0.6.0")
        #expect(store.openFoam?.found == false)
        #expect(store.project == nil)
    }
}
