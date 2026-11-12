import Foundation
import Testing

@testable import VortiaKit

private func fixtureLines() throws -> [Data] {
    let url = try #require(
        Bundle.module.url(forResource: "replies", withExtension: "jsonl", subdirectory: "Fixtures"))
    return try Data(contentsOf: url).split(separator: 0x0A).map { Data($0) }
}

private func object(_ data: Data) throws -> NSDictionary {
    try #require(try JSONSerialization.jsonObject(with: data) as? NSDictionary)
}

@Suite("The engine's wire format")
struct WireTests {
    @Test("every line the engine wrote is understood")
    func everyLineParses() throws {
        for line in try fixtureLines() {
            #expect(Wire.parse(line) != .unreadable, "\(String(decoding: line, as: UTF8.self))")
        }
    }

    @Test("a created project decodes, and its setup goes back exactly as it came")
    func setupRoundTrips() throws {
        let lines = try fixtureLines()
        guard case .reply(1, .success(let data)) = Wire.parse(lines[1]) else {
            Issue.record("not the create reply")
            return
        }
        let project = try JSONDecoder().decode(ProjectInfo.self, from: data)
        #expect(project.name == "Fixture")
        #expect(project.state == "notCreated")
        #expect(project.setup.mesh.cells == [48, 48, 1])
        #expect(project.setup.boundaries.count == 6)
        let theirs = try object(data)["setup"] as? NSDictionary
        let ours = try object(try JSONEncoder().encode(project.setup))
        #expect(ours == theirs)
    }

    @Test("validation carries Reynolds and Courant")
    func validation() throws {
        guard case .reply(2, .success(let data)) = Wire.parse(try fixtureLines()[2]) else {
            Issue.record("not the validate reply")
            return
        }
        let validation = try JSONDecoder().decode(Validation.self, from: data)
        #expect(validation.issues.isEmpty)
        #expect(validation.reynolds == 100)
        #expect(!validation.hasErrors)
    }

    @Test("a refusal is an error with the engine's words")
    func refusal() throws {
        #expect(
            Wire.parse(try fixtureLines()[3])
                == .reply(id: 3, result: .failure(EngineError("there are no results to show yet"))))
    }

    @Test("events: progress, the job and the log")
    func events() throws {
        let lines = try fixtureLines()
        guard case .event(.progress(let progress)) = Wire.parse(lines[4]) else {
            Issue.record("not progress")
            return
        }
        #expect(progress.time == 0.005)
        #expect(progress.residuals.map(\.field) == ["Ux", "p"])
        #expect(progress.residuals[1].final == nil)
        #expect(Wire.parse(lines[5]) == .event(.job(JobEvent(state: "running", phase: "solve"))))
        guard case .event(.job(let done)) = Wire.parse(lines[6]) else {
            Issue.record("not the finished job")
            return
        }
        #expect(done.ok == true)
        #expect(Wire.parse(lines[7]) == .event(.log("Time = 0.005\n")))
    }

    @Test("requests carry their command, an id and exactly their arguments")
    func requests() throws {
        let line = try EngineRequest.line(field: "U", start: [0, 0, 0], end: [0, 1, 0], points: 50)
            .line(id: 9)
        #expect(line.last == 0x0A)
        let sent = try object(line.dropLast())
        #expect(sent["id"] as? Int == 9)
        #expect(sent["cmd"] as? String == "line")
        #expect(sent["field"] as? String == "U")
        #expect(sent["points"] as? Int == 50)
        #expect(sent.count == 6)
        let bare = try object(try EngineRequest.mesh.line(id: 1).dropLast())
        #expect(bare.count == 2)
    }

    @Test("lines split across chunks wait for the rest; blank lines are skipped")
    func splitter() {
        var splitter = LineSplitter()
        #expect(splitter.feed(Data("{\"a\":".utf8)).isEmpty)
        #expect(splitter.feed(Data("1}\n\n  \n{\"b\":2}\n".utf8)).count == 2)
    }
}
