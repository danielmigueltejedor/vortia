import Foundation
import VortiaKit

/// An engine that answers from memory, for screenshots and previews: a solved parametric nozzle
/// whose fields come from the quasi-one-dimensional isentropic solution, with a thin boundary
/// layer laid over it. Nothing here runs OpenFOAM, and runs are refused, so a picture made with
/// it is a picture of the interface, never a result.
public final class DemoEngine: EngineClient, @unchecked Sendable {
    private let lock = NSLock()
    private var points: [[Double]]
    private var bulges: [Double]
    private var nozzle = NozzleDimensions()
    private var flow: JSONValue
    /// Where the contour is narrowest, and its radius there (mm).
    private var throat = (x: 0.0, radius: 1.0)
    /// Whether a project is open: the engine starts with none, so the welcome shows first.
    private var opened = false

    public init() {
        points = DemoEngine.nozzlePoints(NozzleDimensions())
        bulges = Array(repeating: 0, count: points.count)
        flow = .null
        flow = DemoEngine.defaultFlow(points: points)
        findThroat()
    }

    private func findThroat() {
        throat = (0, .infinity)
        for k in 0...400 {
            let x = length * Double(k) / 400
            let r = radius(at: x)
            if r < throat.radius { throat = (x, r) }
        }
    }

    // MARK: Requests

    public func send(_ request: EngineRequest) async throws -> Data {
        lock.lock()
        defer { lock.unlock() }
        let answer: JSONValue
        switch request {
        case .hello:
            answer = .object([
                "protocol": .number(1), "version": .string("demo"),
                "openfoam": DemoEngine.openFoam, "project": opened ? try DemoEngine.project() : .null,
                "running": .bool(false),
            ])
        case .detect:
            answer = DemoEngine.openFoam
        case .create, .open:
            opened = true
            answer = try DemoEngine.project()
        case .save:
            answer = .object(["saved": .bool(true)])
        case .validate:
            answer = .object(["issues": .array([]), "courant": .number(0.4), "reynolds": .number(1000)])
        case .mesh, .solve, .flowMesh, .flowSolve:
            throw EngineError("Demo mode: runs are disabled.")
        case .cancel:
            answer = .object(["cancelling": .bool(true)])
        case .quit:
            answer = .object([:])
        case .results:
            answer = .object([
                "file": .string("/demo/Nozzle/case/VTK/case_4200.vtm"),
                "fields": .array(DemoEngine.fields.map(JSONValue.string)),
                "bounds": .array([
                    .array([0, 0, -halfWedge(at: maxRadius)].map(JSONValue.number)),
                    .array([length / 1000, maxRadius / 1000, halfWedge(at: maxRadius)].map(JSONValue.number)),
                ]),
            ])
        case .statistics(let field, let bins):
            answer = statistics(field, bins: max(1, min(bins, 200)))
        case .line(let field, let start, let end, let count):
            answer = line(field, start: start, end: end, count: max(2, min(count, 2000)))
        case .slice(let field, _, _):
            answer = triangles(field, wedge: false)
        case .surface(let field):
            answer = triangles(field, wedge: true)
        case .sketch:
            answer = .object(["sketch": describe()])
        case .saveSketch(let file):
            let vertices = file["vertices"].array.map { $0.array.compactMap(\.double) }
            guard vertices.count >= 3, vertices.allSatisfy({ $0.count == 2 }) else {
                throw EngineError("a contour needs at least three points")
            }
            let arcs = file["segmentBulges"].array.compactMap(\.double)
            points = vertices
            bulges = arcs.count == vertices.count ? arcs : Array(repeating: 0, count: vertices.count)
            if flow["boundaries"].array.count != points.count { flow = DemoEngine.defaultFlow(points: points) }
            findThroat()
            answer = .object(["sketch": describe()])
        case .nozzle(let dimensions):
            guard dimensions.isValid else {
                throw EngineError("these dimensions do not describe a nozzle (the throat must be the narrowest)")
            }
            nozzle = dimensions
            points = DemoEngine.nozzlePoints(dimensions)
            bulges = Array(repeating: 0, count: points.count)
            flow = DemoEngine.defaultFlow(points: points)
            findThroat()
            answer = .object(["sketch": describe()])
        case .flow:
            answer = .object(["flow": flow])
        case .saveFlow(let edited):
            flow = edited
            answer = .object(["flow": flow])
        case .section(let options, let label):
            answer = section(options, label: label)
        }
        return try JSONEncoder().encode(answer)
    }

    // MARK: The project

    private static let openFoam: JSONValue = .object([
        "found": .bool(true), "fingerprint": .string("OpenFOAM v2606 · bundled"),
        "distribution": .string("openfoam.com"), "root": .string("/Applications/Vortia Native.app"),
    ])

    private static func project() throws -> JSONValue {
        let setup = try JSONDecoder().decode(JSONValue.self, from: Data(cavitySetup.utf8))
        return .object([
            "name": .string("Tobera"), "root": .string("/Users/demo/Vortia/Tobera"),
            "state": .string("solved"), "setup": setup,
        ])
    }

    /// The box case every project also carries, as `caseSetupToJson` writes it.
    private static let cavitySetup = """
        {"domain":{"min":[0,0,0],"max":[0.1,0.1,0.01]},
         "mesh":{"cells":[20,20,1],"grading":[1,1,1],"patches":[
           {"face":0,"name":"fixedWalls","kind":"wall"},{"face":1,"name":"fixedWalls","kind":"wall"},
           {"face":2,"name":"fixedWalls","kind":"wall"},{"face":3,"name":"movingWall","kind":"wall"},
           {"face":4,"name":"frontAndBack","kind":"empty"},{"face":5,"name":"frontAndBack","kind":"empty"}]},
         "material":{"name":"air","nu":0.01},
         "boundaries":[
           {"patch":"movingWall","velocity":"fixedValue","velocityValue":[1,0,0],"pressure":"zeroGradient","pressureValue":0},
           {"patch":"fixedWalls","velocity":"noSlip","velocityValue":[0,0,0],"pressure":"zeroGradient","pressureValue":0},
           {"patch":"frontAndBack","velocity":"empty","velocityValue":[0,0,0],"pressure":"empty","pressureValue":0}],
         "internalField":{"velocity":[0,0,0],"pressure":0},
         "time":{"startTime":0,"endTime":0.5,"deltaT":0.005,"writeInterval":20,"adjustTimeStep":false,"maxCo":1},
         "numerics":{"ddt":"euler","gradScheme":"Gauss linear","divPhiU":"Gauss linear","laplacian":"Gauss linear corrected","pisoCorrectors":2,"toleranceP":1e-6,"toleranceU":1e-5},
         "solver":"icoFoam",
         "turbulence":{"model":"laminar","intensity":0.05,"lengthScale":0.01,"referenceSpeed":1}}
        """

    /// The contour the engine's `makeNozzleProfile` draws: the axis, the outlet, then the wall
    /// back to the inlet, twenty straight pieces a side.
    public static func nozzlePoints(_ d: NozzleDimensions) -> [[Double]] {
        let total = d.convergingLengthMm + d.divergingLengthMm
        let smooth = { (t: Double) in t * t * (3 - 2 * t) }
        var points: [[Double]] = [[0, 0], [total, 0], [total, d.outletDiameterMm / 2]]
        for i in stride(from: 19, through: 0, by: -1) {
            let t = Double(i) / 20
            points.append([
                d.convergingLengthMm + d.divergingLengthMm * t,
                (d.throatDiameterMm + (d.outletDiameterMm - d.throatDiameterMm) * smooth(t)) / 2,
            ])
        }
        for i in stride(from: 19, through: 0, by: -1) {
            let t = Double(i) / 20
            points.append([
                d.convergingLengthMm * t,
                (d.inletDiameterMm + (d.throatDiameterMm - d.inletDiameterMm) * smooth(t)) / 2,
            ])
        }
        return points
    }

    private func describe() -> JSONValue {
        var segments: [JSONValue] = []
        for i in points.indices {
            let a = points[i], b = points[(i + 1) % points.count]
            let chord = hypot(b[0] - a[0], b[1] - a[1])
            let bulge = bulges.indices.contains(i) ? bulges[i] : 0
            let sweep = 4 * atan(bulge)
            let length = bulge == 0 || chord == 0 ? chord : chord * (sweep / 2) / sin(sweep / 2)
            segments.append(.object([
                "length": .number(abs(length)), "bulge": .number(bulge),
                "axis": .bool(a[1] == 0 && b[1] == 0 && bulge == 0),
            ]))
        }
        return .object([
            "points": .array(points.map { .array($0.map(JSONValue.number)) }),
            "closed": .bool(true), "segments": .array(segments),
            "nozzle": .object([
                "inletDiameterMm": .number(nozzle.inletDiameterMm),
                "throatDiameterMm": .number(nozzle.throatDiameterMm),
                "outletDiameterMm": .number(nozzle.outletDiameterMm),
                "convergingLengthMm": .number(nozzle.convergingLengthMm),
                "divergingLengthMm": .number(nozzle.divergingLengthMm),
            ]),
        ])
    }

    /// The desktop app's defaults: the axis on Y = 0, the outlet at the far end, the inlet at
    /// X = 0 and walls between.
    private static func defaultFlow(points: [[Double]]) -> JSONValue {
        let maxX = points.map { $0[0] }.max() ?? 0
        var boundaries: [JSONValue] = []
        for i in points.indices {
            let a = points[i], b = points[(i + 1) % points.count]
            let type: String
            if a[1] == 0 && b[1] == 0 { type = "axis" }
            else if a[0] == maxX && b[0] == maxX { type = "outlet" }
            else if a[0] == 0 && b[0] == 0 { type = "inlet" }
            else { type = "wall" }
            let name = type == "wall" ? "wall" : type
            boundaries.append(.object([
                "name": .string(type == "wall" ? "\(name)_\(i + 1)" : name), "type": .string(type),
                "pressure": .number(type == "inlet" ? 600_000 : 101_325), "temperature": .number(300),
                "viscosityRatio": .number(10),
            ]))
        }
        return .object([
            "mesh": .object(["sizeMm": .number(4), "wallSizeMm": .number(0.8), "wedgeAngle": .number(5)]),
            "material": .object([
                "molecularWeight": .number(28.966), "Cp": .number(1005), "mu": .number(1.8e-5),
                "conductivity": .number(0.025), "turbulence": .string("SpalartAllmaras"),
            ]),
            "initial": .object(["pressure": .number(101_325), "temperature": .number(300)]),
            "solver": .object([
                "iterations": .number(50_000), "writeInterval": .number(100), "maxCo": .number(0.1),
                "logInterval": .number(10), "processes": .number(0),
                "convergence": .object([
                    "enabled": .bool(true), "interval": .number(50), "minimumIterations": .number(500),
                    "stableSamples": .number(5), "massTolerance": .number(0.001),
                    "fieldTolerance": .number(0.001), "equationTolerance": .number(1e-5),
                ]),
            ]),
            "boundaries": .array(boundaries),
        ])
    }

    // MARK: The flow

    static let fields = ["Mach", "T", "U", "p", "rho"]
    private var length: Double { points.map { $0[0] }.max() ?? 1 }
    private var maxRadius: Double { points.map { $0[1] }.max() ?? 1 }
    private func halfWedge(at radius: Double) -> Double { radius / 1000 * tan(2.5 * .pi / 180) }

    /// The wall radius at X (mm), read off the contour's upper side.
    private func radius(at x: Double) -> Double {
        var best = 0.0
        for i in points.indices {
            let a = points[i], b = points[(i + 1) % points.count]
            guard a[1] > 0 || b[1] > 0 else { continue }
            let low = min(a[0], b[0]), high = max(a[0], b[0])
            guard x >= low, x <= high, high > low else { continue }
            let t = (x - a[0]) / (b[0] - a[0])
            best = max(best, a[1] + (b[1] - a[1]) * t)
        }
        return max(best, 1e-6)
    }

    /// Mach from the area ratio: subsonic before the throat, supersonic after it.
    private func mach(at x: Double) -> Double {
        let ratio = max(pow(radius(at: x) / throat.radius, 2), 1)
        let supersonic = x > throat.x
        func area(_ m: Double) -> Double {
            (1 / m) * pow((2 / 2.4) * (1 + 0.2 * m * m), 3)
        }
        var low = supersonic ? 1.0 : 1e-4, high = supersonic ? 6.0 : 1.0
        for _ in 0..<60 {
            let middle = (low + high) / 2
            let tooBig = area(middle) > ratio
            if supersonic { if tooBig { high = middle } else { low = middle } }
            else { if tooBig { low = middle } else { high = middle } }
        }
        return (low + high) / 2
    }

    /// A field's value at X and at a fraction of the local radius.
    private func value(_ field: String, x: Double, eta: Double) -> Double {
        value(field, core: mach(at: x), eta: eta)
    }

    private func value(_ field: String, core: Double, eta: Double) -> Double {
        let layer = 1 - pow(min(max(eta, 0), 1), 14)
        let m = core * (0.35 + 0.65 * layer)
        let p0 = 600_000.0, t0 = 300.0
        let t = t0 / (1 + 0.2 * m * m)
        let p = p0 * pow(1 + 0.2 * core * core, -3.5)
        switch field {
        case "Mach": return m
        case "T": return t
        case "p": return p
        case "rho": return p / (287.05 * t)
        default: return m * sqrt(1.4 * 287.05 * t)
        }
    }

    /// The x–y plane of the wedge, or its two faces and the wall in 3D.
    private func triangles(_ field: String, wedge: Bool) -> JSONValue {
        let nx = 140, ny = 22
        var positions: [Double] = []
        var values: [Double] = []
        let columns = (0...nx).map { i -> (x: Double, radius: Double, mach: Double) in
            let x = length * Double(i) / Double(nx)
            return (x, radius(at: x), mach(at: x))
        }
        func vertex(_ i: Int, _ j: Int, side: Double) {
            let column = columns[i]
            let eta = Double(j) / Double(ny)
            let r = column.radius * eta
            let z = wedge ? side * halfWedge(at: r) * 1000 : 0
            let y = wedge ? r * cos(2.5 * .pi / 180) : r
            positions += [column.x / 1000, y / 1000, z / 1000]
            values.append(value(field, core: column.mach, eta: eta))
        }
        for side in wedge ? [-1.0, 1.0] : [0.0] {
            for i in 0..<nx {
                for j in 0..<ny {
                    for (a, b) in [(i, j), (i + 1, j), (i + 1, j + 1), (i, j), (i + 1, j + 1), (i, j + 1)] {
                        vertex(a, b, side: side)
                    }
                }
            }
        }
        let low = values.min() ?? 0, high = values.max() ?? 1
        return .object([
            "positions": .array(positions.map(JSONValue.number)),
            "values": .array(values.map(JSONValue.number)),
            "range": .array([.number(low), .number(high)]), "field": .string(field),
        ])
    }

    private func samples(_ field: String) -> [Double] {
        var values: [Double] = []
        for i in 0..<120 {
            let core = mach(at: length * (Double(i) + 0.5) / 120)
            for j in 0..<20 { values.append(value(field, core: core, eta: (Double(j) + 0.5) / 20)) }
        }
        return values
    }

    private func statistics(_ field: String, bins: Int) -> JSONValue {
        let values = samples(field)
        let low = values.min() ?? 0, high = values.max() ?? 1
        let mean = values.reduce(0, +) / Double(values.count)
        let variance = values.reduce(0) { $0 + ($1 - mean) * ($1 - mean) } / Double(values.count)
        var counts = Array(repeating: 0, count: bins)
        let width = max(high - low, 1e-30) / Double(bins)
        for v in values { counts[min(bins - 1, Int((v - low) / width))] += 1 }
        return .object([
            "minimum": .number(low), "maximum": .number(high), "mean": .number(mean),
            "standardDeviation": .number(variance.squareRoot()), "integral": .number(mean * 1e-3),
            "measure": .number(1e-3), "cells": .number(Double(values.count)), "excluded": .number(0),
            "histogram": .array(counts.enumerated().map { index, cells -> JSONValue in
                JSONValue.object([
                    "low": .number(low + width * Double(index)),
                    "high": .number(low + width * Double(index + 1)),
                    "measure": .number(Double(cells) / Double(values.count) * 1e-3),
                    "cells": .number(Double(cells)),
                ])
            }),
        ])
    }

    private func line(_ field: String, start: [Double], end: [Double], count: Int) -> JSONValue {
        let total = hypot(end[safe: 0] - start[safe: 0], end[safe: 1] - start[safe: 1])
        var samples: [JSONValue] = []
        for k in 0..<count {
            let t = Double(k) / Double(count - 1)
            let x = (start[safe: 0] + (end[safe: 0] - start[safe: 0]) * t) * 1000
            let y = (start[safe: 1] + (end[safe: 1] - start[safe: 1]) * t) * 1000
            let r = radius(at: x)
            let inside = y <= r
            samples.append(.object([
                "distance": .number(total * t),
                "value": inside ? .number(value(field, x: x, eta: y / r)) : .null,
            ]))
        }
        return .object(["samples": .array(samples)])
    }

    private func section(_ options: SectionOptions, label: String) -> JSONValue {
        let x = min(max(options.position * 1000, 0), length)
        let r = radius(at: x) / 1000
        let fraction = (options.fullAnnulus ? 360 : max(options.wedgeAngle, 5)) / 360
        let area = .pi * r * r * fraction
        let rho = value("rho", x: x, eta: 0.5), u = value("U", x: x, eta: 0.5), p = value("p", x: x, eta: 0.5)
        let mass = rho * u * area * Double(options.normal)
        let metrics: [String: Double] = [
            "area_m2": area, "massFlow_kg_s": mass, "forwardMassFlow_kg_s": abs(mass),
            "reverseMassFlow_kg_s": 0, "volumeFlow_m3_s": u * area, "meanPressure_Pa": p,
            "meanDensity_kg_m3": rho, "meanNormalVelocity_m_s": u, "momentumFlux_N": mass * u,
            "pressureLoad_N": (p - options.referencePressure) * area,
            "normalResultant_N": mass * u + (p - options.referencePressure) * area,
            "kineticPower_W": 0.5 * mass * u * u,
            "normalCoefficient": (mass * u) / max(0.5 * options.referenceDensity * pow(options.referenceSpeed, 2) * options.referenceArea, 1e-30),
            "meanCp": (p - options.referencePressure) / max(0.5 * options.referenceDensity * pow(options.referenceSpeed, 2), 1e-30),
        ]
        let rows = SectionReport.metricKeys.map { "\($0),\(metrics[$0] ?? 0)" }.joined(separator: "\n")
        return .object([
            "report": .object([
                "payload": .object([
                    "label": .string(label), "metrics": .object(metrics.mapValues(JSONValue.number)),
                    "triangles": .number(880),
                ]),
                "sha256": .string(String(repeating: "0", count: 64)),
            ]),
            "html": .string("<h1>\(label)</h1>"), "csv": .string("metric,value\n" + rows),
        ])
    }

    // MARK: A finished run

    /// What a finished compressible run leaves in the store: residuals falling over a few
    /// thousand iterations, the convergence monitor satisfied and the end of the log.
    public static func history() -> [EngineEvent] {
        var events: [EngineEvent] = [.job(JobEvent(state: "running", phase: "solve"))]
        let fields: [(String, Double, Double)] = [
            ("rho", 0.8, 1.6e-6), ("Ux", 0.9, 4.2e-6), ("Uy", 0.7, 6.1e-6), ("e", 0.85, 2.4e-6),
            ("nuTilda", 0.6, 9e-6),
        ]
        for step in stride(from: 10, through: 4200, by: 30) {
            let t = Double(step) / 4200
            let residuals = fields.enumerated().map { index, field in
                let wobble = 1 + 0.25 * sin(Double(step) / 90 + Double(index))
                return Residual(
                    field: field.0, initial: field.1 * pow(field.2 / field.1, t) * wobble, final: nil,
                    iterations: 1)
            }
            events.append(.progress(VortiaKit.Progress(
                time: Double(step), courantMean: 0.04, courantMax: 0.1, residuals: residuals)))
        }
        events.append(.progress(VortiaKit.Progress(
            time: 4200, courantMean: 0.04, courantMax: 0.1, residuals: [],
            convergence: Convergence(
                status: "converged", iteration: 4200, globalMass: 3.1e-5, localMass: 2.2e-4,
                fieldChange: 4.8e-4, equationError: 6.3e-6, stableSamples: 5, requiredSamples: 5))))
        events.append(.log("""
            Time = 4200

            diagonal:  Solving for rho, Initial residual = 0, Final residual = 0, No Iterations 0
            Vortia convergence: mass 3.1e-05, field 4.8e-04, update 6.3e-06 (5/5 stable)
            Flow converged: stopping at iteration 4200
            End

            """))
        events.append(.job(JobEvent(
            state: "finished", phase: "solve", ok: true, summary: "Converged after 4200 iterations")))
        return events
    }
}
