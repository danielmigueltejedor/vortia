#pragma once

/// Laminar flat-plate Blasius boundary layer — analytic / classical correlations.
/// Local skin friction: c_f = 0.664 / √Re_x  (Schlichting, Boundary-Layer Theory).
/// δ_99 / x ≈ 5.0 / √Re_x (Howarth / Schlichting convention for 99% thickness).
/// Similarity profile f'(η) samples from the classical Blasius table (η = y √(U/(νx))).

#include "vortia/physics/Physics.hpp"

#include <array>
#include <cmath>
#include <limits>
#include <vector>

namespace vortia::verification {

struct BlasiusPlate {
    double freestream{1.0};   ///< U∞ [m/s]
    double nu{1.0e-5};        ///< ν [m²/s]
    double length{1.0};       ///< plate length L [m]
    double height{0.2};       ///< domain height [m] (≫ δ at x=L)
    double depth{0.01};       ///< extrusion for 2D empty patches [m]
};

struct Sample1DBlasius {
    double eta{0.0};
    double fPrime{0.0}; ///< u/U∞
};

[[nodiscard]] inline double reynoldsX(const BlasiusPlate& p, double x) {
    return p.freestream * x / p.nu;
}

[[nodiscard]] inline double localSkinFriction(const BlasiusPlate& p, double x) {
    const double rex = reynoldsX(p, x);
    return 0.664 / std::sqrt(rex);
}

/// δ99 = 5 √(ν x / U∞). At x=0 the thickness is zero (not 0/0).
[[nodiscard]] inline double delta99(const BlasiusPlate& p, double x) {
    if (!(x >= 0.0) || !std::isfinite(x) || !(p.freestream > 0.0) || !(p.nu > 0.0))
        return std::numeric_limits<double>::quiet_NaN();
    return 5.0 * std::sqrt(p.nu * x / p.freestream);
}

/// Classical Blasius f'(η) table (Howarth / Schlichting). Units: η dimensionless.
[[nodiscard]] inline const std::vector<Sample1DBlasius>& blasiusFPrimeTable() {
    static const std::vector<Sample1DBlasius> kTable{
        {0.0, 0.0},    {0.2, 0.06641}, {0.4, 0.13277}, {0.6, 0.19894},
        {0.8, 0.26471}, {1.0, 0.32979}, {1.2, 0.39378}, {1.4, 0.45627},
        {1.6, 0.51676}, {1.8, 0.57477}, {2.0, 0.62977}, {2.4, 0.72899},
        {2.8, 0.81152}, {3.2, 0.87609}, {3.6, 0.92333}, {4.0, 0.95552},
        {4.4, 0.97587}, {4.8, 0.98779}, {5.0, 0.99155}, {5.2, 0.99425},
        {5.6, 0.99748}, {6.0, 0.99898}, {8.0, 1.0},
    };
    return kTable;
}

[[nodiscard]] inline const char* blasiusCitation() {
    return "Schlichting H., Boundary-Layer Theory; classical Blasius similarity "
           "(cf=0.664/√Re_x, δ99/x≈5/√Re_x; f′(η) table as in standard BL texts).";
}

/// Builds a laminar flat-plate CaseSetup (incompressible, freestream inlet, no-slip plate).
/// Intended for optional OpenFOAM compare — not claimed verified until native results exist.
[[nodiscard]] inline CaseSetup makeBlasiusCase(const BlasiusPlate& p,
                                               std::array<int, 3> cells = {80, 40, 1}) {
    CaseSetup s;
    s.domain = BoxDomain::fromExtents({0, 0, 0}, {p.length, p.height, p.depth}).value();
    s.mesh.cells = cells;
    // simpleGrading = last/first cell size: >1 packs cells toward y=0 (the plate).
    s.mesh.grading = {1.0, 8.0, 1.0};
    s.mesh.patches = {{
        {BoxDomain::XMin, "inlet", PatchKind::Patch},
        {BoxDomain::XMax, "outlet", PatchKind::Patch},
        {BoxDomain::YMin, "plate", PatchKind::Wall},
        {BoxDomain::YMax, "freestream", PatchKind::Patch},
        {BoxDomain::ZMin, "frontAndBack", PatchKind::Empty},
        {BoxDomain::ZMax, "frontAndBack2", PatchKind::Empty},
    }};
    s.material.name = "blasius-air";
    s.material.nu = KinematicViscosity(p.nu);
    s.boundaries = {
        {"inlet", VelocityBc::FixedValue, {p.freestream, 0, 0}, PressureBc::ZeroGradient, 0.0},
        {"outlet", VelocityBc::ZeroGradient, {}, PressureBc::FixedValue, 0.0},
        {"plate", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"freestream", VelocityBc::FixedValue, {p.freestream, 0, 0}, PressureBc::ZeroGradient, 0.0},
        {"frontAndBack", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
        {"frontAndBack2", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
    };
    s.internalVelocity = {p.freestream, 0, 0};
    s.internalPressure = 0.0;
    s.solver = SolverApp::SimpleFoam;
    s.numerics.ddt = TimeScheme::SteadyState;
    s.time = {0.0, 2000.0, 1.0, 200, false, 1.0};
    return s;
}

} // namespace vortia::verification
