#pragma once

#include "vortia/physics/Physics.hpp"

#include <array>
#include <string_view>
#include <vector>

namespace vortia::verification {

/// Ghia, Ghia & Shin, J. Comput. Phys. 48:387–411 (1982), Re = 100.
/// Tables I (u on x=0.5) and II (v on y=0.5). Coordinates are normalised by
/// cavity side H; velocities by lid speed U. Units: dimensionless.
struct GhiaSample {
    double coordinate; ///< y/H (u table) or x/H (v table)
    double value;      ///< u/U or v/U
};

/// Source: Ghia et al. (1982), Table I, Re=100, u-velocity on vertical centreline.
[[nodiscard]] inline const std::vector<GhiaSample>& ghiaRe100UCenterline() {
    static const std::vector<GhiaSample> samples = {
        {1.0000, 1.00000}, {0.9766, 0.84123}, {0.9688, 0.78871}, {0.9609, 0.73722},
        {0.9531, 0.68717}, {0.8516, 0.23151}, {0.7344, 0.00332},  {0.6172, -0.13641},
        {0.5000, -0.20581}, {0.4531, -0.21090}, {0.2813, -0.15663}, {0.1719, -0.10150},
        {0.1016, -0.06434}, {0.0703, -0.04775}, {0.0625, -0.04192}, {0.0547, -0.03717},
        {0.0000, 0.00000},
    };
    return samples;
}

/// Source: Ghia et al. (1982), Table II, Re=100, v-velocity on horizontal centreline.
[[nodiscard]] inline const std::vector<GhiaSample>& ghiaRe100VCenterline() {
    static const std::vector<GhiaSample> samples = {
        {1.0000, 0.00000}, {0.9688, -0.05906}, {0.9609, -0.07391}, {0.9531, -0.08864},
        {0.9453, -0.10313}, {0.9063, -0.16914}, {0.8594, -0.22445}, {0.8047, -0.24533},
        {0.5000, 0.05454},  {0.2344, 0.17527},  {0.2266, 0.17507},  {0.1563, 0.16077},
        {0.0938, 0.12317},  {0.0781, 0.10890},  {0.0703, 0.10091},  {0.0625, 0.09233},
        {0.0000, 0.00000},
    };
    return samples;
}

[[nodiscard]] inline constexpr std::string_view ghiaCitation() {
    return "U. Ghia, K. N. Ghia, C. T. Shin, "
           "\"High-Re solutions for incompressible flow using the Navier-Stokes equations "
           "and a multigrid method\", Journal of Computational Physics 48:387-411 (1982).";
}

/// Classic lid-driven cavity matching Vortia's default Re≈100 template:
/// H = 0.1 m, U_lid = 1 m/s, ν = 0.001 m²/s ⇒ Re = U H / ν = 100.
[[nodiscard]] inline CaseSetup makeGhiaCavityCase(std::array<int, 3> cells = {64, 64, 1}) {
    CaseSetup s;
    constexpr double H = 0.1;
    constexpr double U = 1.0;
    constexpr double nu = 0.001;
    s.domain = BoxDomain::fromExtents({0, 0, 0}, {H, H, 0.01}).value();
    s.mesh.cells = cells;
    s.mesh.patches = {{
        {BoxDomain::XMin, "fixedWalls", PatchKind::Wall},
        {BoxDomain::XMax, "fixedWalls2", PatchKind::Wall},
        {BoxDomain::YMin, "fixedWalls3", PatchKind::Wall},
        {BoxDomain::YMax, "movingWall", PatchKind::Wall},
        {BoxDomain::ZMin, "frontAndBack", PatchKind::Empty},
        {BoxDomain::ZMax, "frontAndBack2", PatchKind::Empty},
    }};
    s.material.name = "benchmark-fluid";
    s.material.nu = KinematicViscosity(nu);
    s.boundaries = {
        {"fixedWalls", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"fixedWalls2", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"fixedWalls3", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"movingWall", VelocityBc::FixedValue, {U, 0.0, 0.0}, PressureBc::ZeroGradient, 0.0},
        {"frontAndBack", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
        {"frontAndBack2", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
    };
    s.solver = SolverApp::SimpleFoam;
    s.numerics.ddt = TimeScheme::SteadyState;
    s.time = {0.0, 2000.0, 1.0, 100, false, 1.0};
    return s;
}

} // namespace vortia::verification
