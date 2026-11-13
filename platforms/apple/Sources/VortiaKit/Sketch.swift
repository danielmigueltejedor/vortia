import Foundation

/// The five dimensions of the parametric nozzle, in millimetres.
public struct NozzleDimensions: Codable, Equatable, Sendable {
    public var inletDiameterMm = 100.0
    public var throatDiameterMm = 52.0
    public var outletDiameterMm = 120.0
    public var convergingLengthMm = 158.0
    public var divergingLengthMm = 420.0

    public init() {}

    /// The throat is the narrowest section, and every size is positive.
    public var isValid: Bool {
        let all = [inletDiameterMm, throatDiameterMm, outletDiameterMm, convergingLengthMm, divergingLengthMm]
        return all.allSatisfy { $0.isFinite && $0 > 0 && $0 <= 1e6 }
            && throatDiameterMm <= inletDiameterMm && throatDiameterMm <= outletDiameterMm
    }
}

/// The fluid contour as the engine describes it: X along the axis of rotation, Y the radius, mm.
public struct Sketch: Codable, Equatable, Sendable {
    public struct Segment: Codable, Equatable, Sendable {
        public var length: Double
        /// tan(sweep / 4): zero for a straight segment.
        public var bulge: Double
        /// A straight segment on Y = 0 is the axis.
        public var axis: Bool
    }

    public var points: [[Double]]
    public var closed: Bool
    public var segments: [Segment]
    public var nozzle: NozzleDimensions

    public init(points: [[Double]], closed: Bool, segments: [Segment], nozzle: NozzleDimensions) {
        self.points = points
        self.closed = closed
        self.segments = segments
        self.nozzle = nozzle
    }

    /// The `.vsketch.json` the engine reads, with `points` and `bulges` replaced.
    public func file(points: [[Double]], closed: Bool, bulges: [Double]) -> JSONValue {
        let arcs = bulges.contains { $0 != 0 }
        var file: JSONValue = .object([:])
        file["schemaVersion"] = .number(arcs ? 2 : 1)
        file["units"] = .string("mm")
        file["closed"] = .bool(closed)
        file["vertices"] = .array(points.map { .array($0.prefix(2).map(JSONValue.number)) })
        if arcs { file["segmentBulges"] = .array(bulges.map(JSONValue.number)) }
        file["nozzleDimensions"] = .object([
            "inletDiameterMm": .number(nozzle.inletDiameterMm),
            "throatDiameterMm": .number(nozzle.throatDiameterMm),
            "outletDiameterMm": .number(nozzle.outletDiameterMm),
            "convergingLengthMm": .number(nozzle.convergingLengthMm),
            "divergingLengthMm": .number(nozzle.divergingLengthMm),
        ])
        return file
    }

    public var bulges: [Double] { segments.map(\.bulge) }

    /// The ends of segment `index`.
    public func ends(of index: Int) -> ([Double], [Double])? {
        guard points.count >= 2, segments.indices.contains(index) else { return nil }
        return (points[index], points[(index + 1) % points.count])
    }
}

/// What the convergence monitor of vortiaCentralFoam last reported.
public struct Convergence: Codable, Equatable, Sendable {
    public var status: String
    public var iteration: Double?
    public var globalMass: Double?
    public var localMass: Double?
    public var fieldChange: Double?
    public var equationError: Double?
    public var stableSamples: Int
    public var requiredSamples: Int

    public init(
        status: String, iteration: Double?, globalMass: Double?, localMass: Double?,
        fieldChange: Double?, equationError: Double?, stableSamples: Int, requiredSamples: Int
    ) {
        self.status = status
        self.iteration = iteration
        self.globalMass = globalMass
        self.localMass = localMass
        self.fieldChange = fieldChange
        self.equationError = equationError
        self.stableSamples = stableSamples
        self.requiredSamples = requiredSamples
    }
}
