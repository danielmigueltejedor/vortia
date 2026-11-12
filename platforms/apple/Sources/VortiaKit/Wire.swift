import Foundation

/// Client → engine. One JSON object per line, `{"id", "cmd", …}`, as
/// docs/architecture/native-engine.md describes it.
public enum EngineRequest: Equatable, Sendable {
    case hello
    case detect
    case create(path: String, name: String)
    case open(path: String)
    case save(setup: CaseSetup, name: String? = nil)
    case validate(setup: CaseSetup)
    case mesh
    case solve(processes: Int = 1)
    case cancel
    case results
    case statistics(field: String, bins: Int = 24)
    case line(field: String, start: [Double], end: [Double], points: Int = 120)
    case slice(field: String, axis: Int, position: Double)
    case surface(field: String)
    case sketch
    case saveSketch(JSONValue)
    case nozzle(NozzleDimensions)
    case flow
    case saveFlow(JSONValue)
    case flowMesh
    case flowSolve
    case section(options: SectionOptions, label: String)
    case quit

    public var command: String {
        switch self {
        case .hello: "hello"
        case .detect: "detect"
        case .create: "create"
        case .open: "open"
        case .save: "save"
        case .validate: "validate"
        case .mesh: "mesh"
        case .solve: "solve"
        case .cancel: "cancel"
        case .results: "results"
        case .statistics: "statistics"
        case .line: "line"
        case .slice: "slice"
        case .surface: "surface"
        case .sketch: "sketch"
        case .saveSketch: "saveSketch"
        case .nozzle: "nozzle"
        case .flow: "flow"
        case .saveFlow: "saveFlow"
        case .flowMesh: "flowMesh"
        case .flowSolve: "flowSolve"
        case .section: "section"
        case .quit: "quit"
        }
    }

    /// The line to write, with its newline.
    public func line(id: Int) throws -> Data {
        var data = try JSONEncoder().encode(Envelope(id: id, request: self))
        data.append(0x0A)
        return data
    }

    private struct Envelope: Encodable {
        let id: Int
        let request: EngineRequest

        struct Key: CodingKey {
            var stringValue: String
            var intValue: Int? { nil }
            init(_ string: String) { stringValue = string }
            init?(stringValue: String) { self.stringValue = stringValue }
            init?(intValue: Int) { nil }
        }

        func encode(to encoder: any Encoder) throws {
            var c = encoder.container(keyedBy: Key.self)
            try c.encode(id, forKey: Key("id"))
            try c.encode(request.command, forKey: Key("cmd"))
            switch request {
            case .hello, .detect, .mesh, .cancel, .results, .quit, .sketch, .flow, .flowMesh, .flowSolve:
                break
            case .create(let path, let name):
                try c.encode(path, forKey: Key("path"))
                try c.encode(name, forKey: Key("name"))
            case .open(let path):
                try c.encode(path, forKey: Key("path"))
            case .save(let setup, let name):
                try c.encode(setup, forKey: Key("setup"))
                try c.encodeIfPresent(name, forKey: Key("name"))
            case .validate(let setup):
                try c.encode(setup, forKey: Key("setup"))
            case .solve(let processes):
                try c.encode(processes, forKey: Key("processes"))
            case .statistics(let field, let bins):
                try c.encode(field, forKey: Key("field"))
                try c.encode(bins, forKey: Key("bins"))
            case .line(let field, let start, let end, let points):
                try c.encode(field, forKey: Key("field"))
                try c.encode(start, forKey: Key("start"))
                try c.encode(end, forKey: Key("end"))
                try c.encode(points, forKey: Key("points"))
            case .slice(let field, let axis, let position):
                try c.encode(field, forKey: Key("field"))
                try c.encode(axis, forKey: Key("axis"))
                try c.encode(position, forKey: Key("position"))
            case .surface(let field):
                try c.encode(field, forKey: Key("field"))
            case .saveSketch(let sketch):
                try c.encode(sketch, forKey: Key("sketch"))
            case .nozzle(let dimensions):
                try c.encode(dimensions, forKey: Key("dimensions"))
            case .saveFlow(let flow):
                try c.encode(flow, forKey: Key("flow"))
            case .section(let options, let label):
                try c.encode(options, forKey: Key("options"))
                try c.encode(label, forKey: Key("label"))
            }
        }
    }
}

// MARK: Answers

public struct OpenFoamStatus: Codable, Equatable, Sendable {
    public var found: Bool
    public var fingerprint: String?
    public var distribution: String?
    public var root: String?
    public var error: String?
}

public struct Hello: Codable, Equatable, Sendable {
    public var `protocol`: Int
    public var version: String
    public var openfoam: OpenFoamStatus
    public var project: ProjectInfo?
    public var running: Bool
}

public struct Issue: Codable, Equatable, Sendable, Hashable {
    /// error or warning.
    public var severity: String
    public var message: String
    public var isError: Bool { severity == "error" }
}

public struct Validation: Codable, Equatable, Sendable {
    public var issues: [Issue]
    public var courant: Double?
    public var reynolds: Double?
    public var hasErrors: Bool { issues.contains(where: \.isError) }
}

public struct ResultsInfo: Codable, Equatable, Sendable {
    public var file: String
    public var fields: [String]
    /// `[min, max]`, three numbers each.
    public var bounds: [[Double]]
}

public struct FieldStatistics: Codable, Equatable, Sendable {
    public struct Bin: Codable, Equatable, Sendable {
        public var low: Double?
        public var high: Double?
        public var measure: Double?
        public var cells: Int
    }

    public var minimum: Double?
    public var maximum: Double?
    public var mean: Double?
    public var standardDeviation: Double?
    public var integral: Double?
    public var measure: Double?
    public var cells: Int
    public var excluded: Int
    public var histogram: [Bin]
}

public struct LineProfile: Codable, Equatable, Sendable {
    public struct Sample: Codable, Equatable, Sendable {
        public var distance: Double?
        public var value: Double?
    }

    public var samples: [Sample]
}

/// Triangles with a value at every vertex, unindexed: three positions (nine floats) and three
/// values per triangle.
public struct FieldTriangles: Codable, Equatable, Sendable {
    public var positions: [Float]
    public var values: [Float]
    public var range: [Double]?
    public var field: String

    public var triangleCount: Int { positions.count / 9 }
}

/// Where a section cuts the domain and what it is measured against, as `SectionOptions` in
/// `src/visualization` reads it.
public struct SectionOptions: Codable, Equatable, Sendable {
    public var axis = 0
    /// 1 or −1: which way along the axis counts as forward.
    public var normal = 1
    public var position = 0.0
    public var referencePressure = 101_325.0
    public var constantDensity = 1.225
    public var referenceDensity = 1.225
    public var referenceSpeed = 1.0
    public var referenceArea = 1.0
    public var useDensityField = false
    /// OpenFOAM's incompressible solvers write p/ρ (m²/s²), not pascals.
    public var kinematicPressure = true
    public var fullAnnulus = false
    public var wedgeAngle = 0.0

    public init() {}
}

/// The engineering report of one section: its metrics, and the same report as HTML and CSV.
public struct SectionReport: Decodable, Equatable, Sendable {
    public struct Report: Decodable, Equatable, Sendable {
        public struct Payload: Decodable, Equatable, Sendable {
            public var label: String
            public var metrics: [String: Double]
            public var triangles: Int
        }
        public var payload: Payload
        public var sha256: String
    }

    public var report: Report
    public var html: String
    public var csv: String
    /// The whole report as the desktop app saves it.
    public var json: Data

    private enum Keys: String, CodingKey { case report, html, csv }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        report = try c.decode(Report.self, forKey: .report)
        html = try c.decode(String.self, forKey: .html)
        csv = try c.decode(String.self, forKey: .csv)
        json = Data()
    }

    /// Keeps the report's own JSON, which the typed view above only partly reads.
    public static func decode(_ data: Data) throws -> SectionReport {
        var decoded = try JSONDecoder().decode(SectionReport.self, from: data)
        if let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            let report = object["report"]
        {
            decoded.json = try JSONSerialization.data(
                withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
        }
        return decoded
    }

    /// The metrics in the order the desktop app lists them.
    public static let metricKeys = [
        "area_m2", "massFlow_kg_s", "forwardMassFlow_kg_s", "reverseMassFlow_kg_s",
        "volumeFlow_m3_s", "meanPressure_Pa", "meanDensity_kg_m3", "meanNormalVelocity_m_s",
        "momentumFlux_N", "pressureLoad_N", "normalResultant_N", "kineticPower_W",
        "normalCoefficient", "meanCp",
    ]
}

// MARK: Events

public struct Residual: Codable, Equatable, Sendable, Hashable {
    public var field: String
    public var initial: Double?
    public var final: Double?
    public var iterations: Int

    public init(field: String, initial: Double?, final: Double?, iterations: Int) {
        self.field = field
        self.initial = initial
        self.final = final
        self.iterations = iterations
    }
}

public struct Progress: Codable, Equatable, Sendable {
    public var time: Double?
    public var courantMean: Double?
    public var courantMax: Double?
    public var residuals: [Residual]
    /// Only from the compressible solver.
    public var convergence: Convergence?

    public init(
        time: Double?, courantMean: Double?, courantMax: Double?, residuals: [Residual],
        convergence: Convergence? = nil
    ) {
        self.time = time
        self.courantMean = courantMean
        self.courantMax = courantMax
        self.residuals = residuals
        self.convergence = convergence
    }
}

public struct JobEvent: Codable, Equatable, Sendable {
    /// running or finished.
    public var state: String
    /// mesh or solve.
    public var phase: String
    public var ok: Bool?
    public var summary: String?

    public init(state: String, phase: String, ok: Bool? = nil, summary: String? = nil) {
        self.state = state
        self.phase = phase
        self.ok = ok
        self.summary = summary
    }
}

public enum EngineEvent: Equatable, Sendable {
    case ready(protocol: Int)
    case log(String)
    case progress(Progress)
    case job(JobEvent)
}

/// One line from the engine: an answer to a request, or an event.
public enum EngineLine: Equatable, Sendable {
    case reply(id: Int, result: Result<Data, EngineError>)
    case event(EngineEvent)
    case unreadable
}

public struct EngineError: Error, Equatable, Sendable, LocalizedError {
    public var message: String
    public init(_ message: String) { self.message = message }
    public var errorDescription: String? { message }
}

public enum Wire {
    /// Reads one line. A reply keeps its `result` as raw JSON for the caller to decode into
    /// the type it asked for.
    public static func parse(_ line: Data) -> EngineLine {
        guard let object = (try? JSONSerialization.jsonObject(with: line)) as? [String: Any] else {
            return .unreadable
        }
        if let event = object["event"] as? String {
            return parseEvent(event, object, line) ?? .unreadable
        }
        guard let id = object["id"] as? Int else { return .unreadable }
        if object["ok"] as? Bool == true {
            let result = object["result"] ?? [String: Any]()
            guard
                let data = try? JSONSerialization.data(
                    withJSONObject: result, options: [.fragmentsAllowed])
            else { return .unreadable }
            return .reply(id: id, result: .success(data))
        }
        let message = object["error"] as? String ?? "the engine refused the request"
        return .reply(id: id, result: .failure(EngineError(message)))
    }

    private static func parseEvent(_ name: String, _ object: [String: Any], _ line: Data)
        -> EngineLine?
    {
        let decoder = JSONDecoder()
        switch name {
        case "ready":
            return .event(.ready(protocol: object["protocol"] as? Int ?? 0))
        case "log":
            return (object["text"] as? String).map { .event(.log($0)) }
        case "progress":
            return (try? decoder.decode(Progress.self, from: line)).map { .event(.progress($0)) }
        case "job":
            return (try? decoder.decode(JobEvent.self, from: line)).map { .event(.job($0)) }
        default:
            return nil
        }
    }
}

/// Splits a byte stream into lines, keeping a partial line for the next chunk.
public struct LineSplitter: Sendable {
    private var pending = Data()

    public init() {}

    public mutating func feed(_ chunk: Data) -> [Data] {
        pending.append(chunk)
        var lines: [Data] = []
        while let newline = pending.firstIndex(of: 0x0A) {
            let line = pending[pending.startIndex..<newline]
            if !line.allSatisfy({ $0 == 0x20 || $0 == 0x0D || $0 == 0x09 }) {
                lines.append(Data(line))
            }
            pending.removeSubrange(pending.startIndex...newline)
        }
        return lines
    }
}
