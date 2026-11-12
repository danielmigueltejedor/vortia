import Foundation

/// The case as `caseSetupToJson` writes it: every key, so a setup read from the engine and
/// written back loses nothing. The engine is the one that validates; this only carries.
public struct CaseSetup: Codable, Equatable, Sendable {
    public struct Domain: Codable, Equatable, Sendable {
        public var min: [Double]
        public var max: [Double]
    }

    public struct Patch: Codable, Equatable, Sendable, Identifiable {
        /// 0…5: −x, +x, −y, +y, −z, +z.
        public var face: Int
        public var name: String
        /// wall, patch, empty or symmetry.
        public var kind: String
        public var id: Int { face }
    }

    public struct Mesh: Codable, Equatable, Sendable {
        public var cells: [Int]
        public var grading: [Double]
        public var patches: [Patch]
    }

    public struct Material: Codable, Equatable, Sendable {
        public var name: String
        /// Kinematic viscosity, m²/s.
        public var nu: Double
    }

    public struct Boundary: Codable, Equatable, Sendable, Identifiable {
        public var patch: String
        /// noSlip, fixedValue, zeroGradient, symmetry or empty.
        public var velocity: String
        public var velocityValue: [Double]
        /// zeroGradient, fixedValue, symmetry or empty.
        public var pressure: String
        public var pressureValue: Double
        public var id: String { patch }
    }

    public struct InternalField: Codable, Equatable, Sendable {
        public var velocity: [Double]
        public var pressure: Double
    }

    public struct Time: Codable, Equatable, Sendable {
        public var startTime: Double
        public var endTime: Double
        public var deltaT: Double
        public var writeInterval: Int
        public var adjustTimeStep: Bool
        public var maxCo: Double
    }

    public struct Numerics: Codable, Equatable, Sendable {
        /// euler, backward or steadyState.
        public var ddt: String
        public var gradScheme: String
        public var divPhiU: String
        public var laplacian: String
        public var pisoCorrectors: Int
        public var toleranceP: Double
        public var toleranceU: Double
    }

    public struct Turbulence: Codable, Equatable, Sendable {
        /// laminar or kOmegaSST.
        public var model: String
        public var intensity: Double
        public var lengthScale: Double
        public var referenceSpeed: Double
    }

    public var domain: Domain
    public var mesh: Mesh
    public var material: Material
    public var boundaries: [Boundary]
    public var internalField: InternalField
    public var time: Time
    public var numerics: Numerics
    /// icoFoam or simpleFoam.
    public var solver: String
    public var turbulence: Turbulence

    /// The domain's size along each axis.
    public var size: [Double] {
        (0..<3).map { domain.max[safe: $0] - domain.min[safe: $0] }
    }

    /// Whether the solver marches in time or iterates to a steady state.
    public var isSteady: Bool { solver == "simpleFoam" || numerics.ddt == "steadyState" }
}

extension Array where Element == Double {
    /// Zero for a missing component, so a short vector reads as a point at the origin.
    public subscript(safe index: Int) -> Double {
        indices.contains(index) ? self[index] : 0
    }
}

extension Array where Element == Int {
    /// One for a missing component: a cell count never reads as zero.
    public subscript(safe index: Int) -> Int {
        indices.contains(index) ? self[index] : 1
    }
}

/// What lives where, as the engine reports it.
public struct ProjectInfo: Codable, Equatable, Sendable {
    public var name: String
    public var root: String
    /// notCreated, configured, meshed or solved.
    public var state: String
    public var setup: CaseSetup
}
