#pragma once

#include "vortia/physics/Physics.hpp"

#include <array>
#include <cmath>

namespace vortia::verification {

/// Fully developed laminar channel between y=0 and y=H (kinematic pressure).
/// Momentum balance: 0 = -dp/dx + nu d²u/dy²  ⇒  u(y) = (-dp/dx)/(2ν) · y·(H-y).
struct PoiseuilleChannel {
    double height{0.01};           ///< H [m]
    double length{0.1};            ///< L [m] (streamwise)
    double depth{0.01};            ///< extrusion for 2D empty patches [m]
    double nu{1.0e-3};             ///< kinematic viscosity [m²/s]
    double pressureInlet{0.1};     ///< kinematic p at x=0 [m²/s²]
    double pressureOutlet{0.0};    ///< kinematic p at x=L [m²/s²]
};

[[nodiscard]] inline double pressureGradient(const PoiseuilleChannel& c) {
    return (c.pressureOutlet - c.pressureInlet) / c.length; // typically negative
}

[[nodiscard]] inline double analyticalU(const PoiseuilleChannel& c, double y) {
    const double dpdx = pressureGradient(c);
    return (-dpdx) / (2.0 * c.nu) * y * (c.height - y);
}

[[nodiscard]] inline double analyticalUmax(const PoiseuilleChannel& c) {
    return analyticalU(c, 0.5 * c.height);
}

/// Volumetric flow rate per unit depth for the parabolic profile [m²/s].
[[nodiscard]] inline double analyticalFlowPerDepth(const PoiseuilleChannel& c) {
    return (2.0 / 3.0) * analyticalUmax(c) * c.height;
}

/// Builds a laminar incompressible CaseSetup for pressure-driven 2D Poiseuille.
[[nodiscard]] inline CaseSetup makePoiseuilleCase(const PoiseuilleChannel& c,
                                                  std::array<int, 3> cells = {40, 20, 1}) {
    CaseSetup s;
    s.domain = BoxDomain::fromExtents({0, 0, 0}, {c.length, c.height, c.depth}).value();
    s.mesh.cells = cells;
    s.mesh.patches = {{
        {BoxDomain::XMin, "inlet", PatchKind::Patch},
        {BoxDomain::XMax, "outlet", PatchKind::Patch},
        {BoxDomain::YMin, "bottomWall", PatchKind::Wall},
        {BoxDomain::YMax, "topWall", PatchKind::Wall},
        {BoxDomain::ZMin, "frontAndBack", PatchKind::Empty},
        {BoxDomain::ZMax, "frontAndBack2", PatchKind::Empty},
    }};
    s.material.name = "benchmark-fluid";
    s.material.nu = KinematicViscosity(c.nu);
    s.boundaries = {
        {"inlet", VelocityBc::ZeroGradient, {}, PressureBc::FixedValue, c.pressureInlet},
        {"outlet", VelocityBc::ZeroGradient, {}, PressureBc::FixedValue, c.pressureOutlet},
        {"bottomWall", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"topWall", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"frontAndBack", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
        {"frontAndBack2", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
    };
    s.internalVelocity = {};
    s.internalPressure = 0.5 * (c.pressureInlet + c.pressureOutlet);
    // Steady SIMPLE is appropriate for fully developed channel verification.
    s.solver = SolverApp::SimpleFoam;
    s.numerics.ddt = TimeScheme::SteadyState;
    s.time = {0.0, 500.0, 1.0, 50, false, 1.0};
    return s;
}

} // namespace vortia::verification
