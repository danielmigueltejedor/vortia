#pragma once

#include "vortia/physics/Physics.hpp"

#include <array>
#include <cmath>
#include <limits>

namespace vortia::verification {

/// Zero-pressure-gradient turbulent boundary layer on a smooth flat plate,
/// turbulent from the leading edge. Incompressible, kinematic pressure.
struct TurbulentPlate {
    double length{10.0};     ///< L [m]
    double height{2.0};      ///< domain height above the plate [m]
    double depth{0.1};       ///< extrusion for the 2D empty patches [m]
    double freestream{10.0}; ///< U∞ [m/s]
    double nu{1.0e-5};       ///< kinematic viscosity [m²/s]
    double intensity{0.01};  ///< free-stream turbulence intensity
};

[[nodiscard]] inline double plateReynolds(const TurbulentPlate& p) {
    return p.freestream * p.length / p.nu;
}

/// Mean skin-friction (drag) coefficient of one side of the plate,
/// Prandtl–Schlichting: C_D = 0.455 / (log10 Re_L)^2.58, for 1e6 ≲ Re_L ≲ 1e9.
/// An empirical correlation (a few per cent scatter), not an exact solution.
[[nodiscard]] inline double schlichtingDragCoefficient(double reynolds) {
    if (!std::isfinite(reynolds) || reynolds < 1.0e5)
        return std::numeric_limits<double>::quiet_NaN();
    return 0.455 / std::pow(std::log10(reynolds), 2.58);
}

/// Friction force per unit density on the plate [m⁴/s²]: C_D · ½ U∞² · L · depth.
[[nodiscard]] inline double schlichtingPlateForce(const TurbulentPlate& p) {
    return schlichtingDragCoefficient(plateReynolds(p)) * 0.5 * p.freestream * p.freestream *
           p.length * p.depth;
}

[[nodiscard]] inline const char* turbulentPlateCitation() {
    return "Schlichting H., Gersten K., Boundary-Layer Theory, 9th ed., Springer (2017): "
           "Prandtl-Schlichting skin-friction law for the smooth flat plate, "
           "C_D = 0.455/(log10 Re_L)^2.58.";
}

/// Steady k-ω SST case with wall functions. The wall-normal grading places the
/// first cell centre in the logarithmic layer (y+ of order 50–100 for the
/// default plate), as wall functions require.
[[nodiscard]] inline CaseSetup makeTurbulentPlateCase(const TurbulentPlate& p,
                                                      std::array<int, 3> cells = {200, 60, 1}) {
    CaseSetup s;
    s.domain = BoxDomain::fromExtents({0, 0, 0}, {p.length, p.height, p.depth}).value();
    s.mesh.cells = cells;
    // simpleGrading = last/first cell size: > 1 packs cells toward the plate.
    s.mesh.grading = {1.0, 100.0, 1.0};
    s.mesh.patches = {{
        {BoxDomain::XMin, "inlet", PatchKind::Patch},
        {BoxDomain::XMax, "outlet", PatchKind::Patch},
        {BoxDomain::YMin, "plate", PatchKind::Wall},
        {BoxDomain::YMax, "freestream", PatchKind::Patch},
        {BoxDomain::ZMin, "frontAndBack", PatchKind::Empty},
        {BoxDomain::ZMax, "frontAndBack2", PatchKind::Empty},
    }};
    s.material.name = "benchmark-air";
    s.material.nu = KinematicViscosity(p.nu);
    s.boundaries = {
        {"inlet", VelocityBc::FixedValue, {p.freestream, 0, 0}, PressureBc::ZeroGradient, 0.0},
        {"outlet", VelocityBc::ZeroGradient, {}, PressureBc::FixedValue, 0.0},
        {"plate", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        // Far-field boundary: lets the displaced fluid leave, at ambient pressure.
        {"freestream", VelocityBc::ZeroGradient, {}, PressureBc::FixedValue, 0.0},
        {"frontAndBack", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
        {"frontAndBack2", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
    };
    s.internalVelocity = {p.freestream, 0, 0};
    s.internalPressure = 0.0;
    s.solver = SolverApp::SimpleFoam;
    s.numerics.ddt = TimeScheme::SteadyState;
    s.numerics.divPhiU = "bounded Gauss linearUpwind grad(U)";
    s.time = {0.0, 3000.0, 1.0, 3000, false, 1.0};
    s.turbulence.model = TurbulenceModel::KOmegaSST;
    s.turbulence.intensity = p.intensity;
    // Small free-stream eddies: viscosity ratio of order one at the inlet.
    s.turbulence.lengthScale = 1.0e-3;
    return s;
}

} // namespace vortia::verification
