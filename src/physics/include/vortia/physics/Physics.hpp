#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/core/Units.hpp"
#include "vortia/core/Vec3.hpp"
#include "vortia/mesh/BlockMeshDict.hpp"

#include <array>
#include <string>
#include <vector>

namespace vortia {

// ---------------------------------------------------------------------------
// Material
// ---------------------------------------------------------------------------

/// Newtonian incompressible fluid. For the laminar incompressible solvers of
/// the MVP, kinematic viscosity nu is the only transport property OpenFOAM
/// requires (pressure is kinematic, m²/s²).
struct FluidMaterial {
    std::string name{"air"};
    KinematicViscosity nu{1.5e-5}; ///< m²/s
};

// ---------------------------------------------------------------------------
// Boundary conditions
// ---------------------------------------------------------------------------

enum class VelocityBc { NoSlip, FixedValue, ZeroGradient, Symmetry, Empty };
enum class PressureBc { ZeroGradient, FixedValue, Symmetry, Empty };

[[nodiscard]] std::string_view toOpenFoam(VelocityBc bc);
[[nodiscard]] std::string_view toOpenFoam(PressureBc bc);

/// Boundary condition attached to a mesh patch by name.
struct PatchBoundary {
    std::string patchName;
    VelocityBc velocity{VelocityBc::NoSlip};
    Vec3 velocityValue{};              ///< m/s, used when velocity == FixedValue.
    PressureBc pressure{PressureBc::ZeroGradient};
    double pressureValue{0.0};         ///< m²/s² (kinematic pressure), for FixedValue.
};

// ---------------------------------------------------------------------------
// Numerical configuration
// ---------------------------------------------------------------------------

enum class TimeScheme { Euler, Backward, SteadyState };
enum class SolverApp { IcoFoam, SimpleFoam };

struct NumericsConfig {
    TimeScheme ddt{TimeScheme::Euler};
    std::string gradScheme{"Gauss linear"};
    std::string divPhiU{"Gauss linear"};
    std::string laplacian{"Gauss linear corrected"};
    int pisoCorrectors{2};
    double toleranceP{1e-6};
    double toleranceU{1e-7};
    int maxIterationsP{1000};
};

struct TimeConfig {
    double startTime{0.0};      ///< s
    double endTime{0.5};        ///< s
    double deltaT{0.001};       ///< s
    int writeInterval{100};     ///< in time steps
    bool adjustTimeStep{false}; ///< CFL-limited automatic stepping
    double maxCo{1.0};
};

// ---------------------------------------------------------------------------
// Case setup (validated aggregate)
// ---------------------------------------------------------------------------

enum class Severity { Warning, Error };

struct Issue {
    Severity severity;
    std::string message;
};

/// Everything required to write and run a laminar incompressible OpenFOAM
/// case on a box domain: geometry + mesh + material + boundaries + numerics.
struct CaseSetup {
    BoxDomain domain;
    BlockMeshParams mesh;
    FluidMaterial material;
    std::vector<PatchBoundary> boundaries; ///< One per mesh patch name.
    Vec3 internalVelocity{};               ///< initial internal field, m/s
    double internalPressure{0.0};          ///< m²/s²
    TimeConfig time;
    NumericsConfig numerics;
    SolverApp solver{SolverApp::IcoFoam};

    /// Structural + physical validation. Errors block case generation;
    /// warnings are surfaced to the user (e.g. Courant estimate).
    [[nodiscard]] std::vector<Issue> validate() const;

    /// Estimated Courant number given the largest fixed/inlet velocity,
    /// the smallest cell size and deltaT. Returns 0 for steady-state.
    [[nodiscard]] double estimateMaxCourant() const;

    /// Estimated Reynolds number based on the largest prescribed velocity
    /// magnitude and the smallest domain dimension. 0 if no driving velocity.
    [[nodiscard]] double estimateReynolds() const;
};

} // namespace vortia
