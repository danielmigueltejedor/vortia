import Foundation
import Testing

@testable import VortiaKit

@Suite("Contour and flow documents")
struct SketchTests {
    @Test("JSONValue carries every key it does not know, and edits the ones it does")
    func jsonValue() throws {
        let text = #"{"mesh":{"sizeMm":20,"partitions":[]},"boundaries":[{"name":"axis","type":"axis"}],"x":null}"#
        var value = try JSONDecoder().decode(JSONValue.self, from: Data(text.utf8))
        value["mesh"]["sizeMm"] = .number(12)
        value["boundaries"][0]["name"] = .string("eje")
        #expect(value["mesh"]["sizeMm"].double == 12)
        #expect(value["boundaries"][0]["name"].string == "eje")
        #expect(value["mesh"]["partitions"] == .array([]))
        #expect(value["x"] == .null)
        let again = try JSONDecoder().decode(JSONValue.self, from: try JSONEncoder().encode(value))
        #expect(again == value)
    }

    @Test("a contour writes schema 1 without arcs and 2 with them")
    func file() {
        let sketch = Sketch(
            points: [[0, 0], [10, 0], [10, 5]], closed: true,
            segments: Array(repeating: .init(length: 1, bulge: 0, axis: false), count: 3),
            nozzle: NozzleDimensions())
        let straight = sketch.file(points: sketch.points, closed: true, bulges: [0, 0, 0])
        #expect(straight["schemaVersion"].double == 1)
        #expect(straight["segmentBulges"] == .null)
        let bent = sketch.file(points: sketch.points, closed: true, bulges: [0, 0.2, 0])
        #expect(bent["schemaVersion"].double == 2)
        #expect(bent["segmentBulges"].array.count == 3)
        #expect(bent["units"].string == "mm")
    }

    @Test("a nozzle is valid only when the throat is the narrowest")
    func nozzle() {
        var n = NozzleDimensions()
        #expect(n.isValid)
        n.throatDiameterMm = 200
        #expect(!n.isValid)
    }
}
