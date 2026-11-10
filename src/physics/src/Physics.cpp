#include "vortia/physics/Physics.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <set>

namespace vortia {

std::string_view toOpenFoam(VelocityBc bc) {
    switch (bc) {
    case VelocityBc::NoSlip: return "noSlip";
    case VelocityBc::FixedValue: return "fixedValue";
    case VelocityBc::ZeroGradient: return "zeroGradient";
    case VelocityBc::Symmetry: return "symmetry";
    case VelocityBc::Empty: return "empty";
    }
    return "zeroGradient";
}

std::string_view toOpenFoam(PressureBc bc) {
    switch (bc) {
    case PressureBc::ZeroGradient: return "zeroGradient";
    case PressureBc::FixedValue: return "fixedValue";
    case PressureBc::Symmetry: return "symmetry";
    case PressureBc::Empty: return "empty";
    }
    return "zeroGradient";
}

namespace {

[[nodiscard]] double maxPrescribedSpeed(const CaseSetup& setup) {
    double maxSpeed = setup.internalVelocity.norm();
    for (const auto& bc : setup.boundaries)
        if (bc.velocity == VelocityBc::FixedValue || bc.velocity == VelocityBc::NoSlip)
            maxSpeed = std::max(maxSpeed, bc.velocityValue.norm());
    return maxSpeed;
}

} // namespace

double CaseSetup::estimateMaxCourant() const {
    if (numerics.ddt == TimeScheme::SteadyState || time.deltaT <= 0.0)
        return 0.0;
    const double speed = maxPrescribedSpeed(*this);
    if (speed <= 0.0)
        return 0.0;
    const Vec3 size = domain.size();
    double minDx = std::numeric_limits<double>::max();
    for (int axis = 0; axis < 3; ++axis) {
        const int count = mesh.cells[axis];
        const double length = size[axis];
        const double grade = mesh.grading[axis];
        if (count < 1 || !(length > 0.0) || !(grade > 0.0) || !std::isfinite(grade))
            continue;
        double smallest = length / count;
        if (count > 1 && std::abs(std::log(grade)) > 1e-10) {
            // simpleGrading is (last cell width) / (first cell width).
            // Actual cell sizes form a geometric progression.
            const double logQ = std::log(grade) / (count - 1);
            const double first = length * std::expm1(logQ) /
                                 std::expm1(count * logQ);
            smallest = first * std::min(1.0, grade);
        }
        minDx = std::min(minDx, smallest);
    }
    if (!std::isfinite(minDx) || minDx <= 0.0)
        return 0.0;
    return speed * time.deltaT / minDx;
}

double CaseSetup::estimateReynolds() const {
    const double speed = maxPrescribedSpeed(*this);
    const double nu = material.nu.siValue();
    if (speed <= 0.0 || nu <= 0.0)
        return 0.0;
    // Heuristic: the characteristic length is the LARGEST domain extent —
    // for driven internal flows (e.g. lid cavity) the cavity side is the
    // physically meaningful scale; the thin extrusion of a 2D case is not.
    const Vec3 size = domain.size();
    const double length = std::max({size.x, size.y, size.z});
    if (length <= 0.0)
        return 0.0;
    return speed * length / nu;
}

std::vector<Issue> CaseSetup::validate() const {
    std::vector<Issue> issues;
    auto error = [&issues](std::string msg) {
        issues.push_back({Severity::Error, std::move(msg)});
    };
    auto warning = [&issues](std::string msg) {
        issues.push_back({Severity::Warning, std::move(msg)});
    };

    // --- Domain ---
    if (!domain.isValid())
        error("computational domain is not valid (max must exceed min on every axis)");

    // --- Mesh ---
    for (int axis = 0; axis < 3; ++axis) {
        if (mesh.cells[axis] < 1)
            error("mesh cell count must be >= 1 in all directions");
        if (!std::isfinite(mesh.grading[axis]) || mesh.grading[axis] <= 0.0)
            error("mesh grading must be finite and positive");
    }

    // --- Material ---
    if (material.nu.siValue() <= 0.0 || !std::isfinite(material.nu.siValue()))
        error("kinematic viscosity nu must be positive and finite");

    // --- Time ---
    if (numerics.ddt != TimeScheme::SteadyState) {
        if (!(time.deltaT > 0.0))
            error("deltaT must be positive");
        if (!(time.endTime > time.startTime))
            error("endTime must be greater than startTime");
        if (time.writeInterval < 1)
            error("writeInterval must be >= 1");
        if (time.adjustTimeStep && !(time.maxCo > 0.0))
            error("maxCo must be positive when adjustTimeStep is enabled");
    }

    // --- Boundaries: every mesh patch must have exactly one BC ---
    std::set<std::string> patchNames;
    for (const auto& p : mesh.patches)
        patchNames.insert(p.name);
    std::set<std::string> bcNames;
    for (const auto& bc : boundaries) {
        if (!patchNames.contains(bc.patchName))
            error("boundary condition references unknown patch '" + bc.patchName + "'");
        if (!bcNames.insert(bc.patchName).second)
            error("duplicate boundary condition for patch '" + bc.patchName + "'");
        if (bc.velocity == VelocityBc::FixedValue) {
            const double s = bc.velocityValue.norm();
            if (!std::isfinite(s))
                error("fixed velocity on patch '" + bc.patchName + "' is not finite");
        }
        // BC kinds must be consistent with the patch base type in the mesh.
        for (const auto& p : mesh.patches) {
            if (p.name != bc.patchName)
                continue;
            if (p.kind == PatchKind::Empty &&
                (bc.velocity != VelocityBc::Empty || bc.pressure != PressureBc::Empty))
                error("patch '" + p.name + "' is 'empty' in the mesh: U and p BCs must be 'empty'");
            if (p.kind == PatchKind::Wall &&
                (bc.velocity == VelocityBc::Empty || bc.pressure == PressureBc::Empty))
                error("patch '" + p.name + "' is a wall: 'empty' BCs are not allowed");
        }
    }
    for (const auto& p : mesh.patches)
        if (!bcNames.contains(p.name))
            error("mesh patch '" + p.name + "' has no boundary condition");

    // --- Physical heuristics (warnings, never blockers) ---
    const double co = estimateMaxCourant();
    if (co > 1.0 && !time.adjustTimeStep)
        warning("estimated maximum Courant number is " + std::to_string(co) +
                " (> 1): transient accuracy/stability at risk with explicit PISO; "
                "reduce deltaT or enable adjustTimeStep");
    if (numerics.ddt == TimeScheme::SteadyState && solver == SolverApp::IcoFoam)
        error("icoFoam is a transient solver: use simpleFoam for steadyState");
    if (numerics.ddt != TimeScheme::SteadyState && solver == SolverApp::SimpleFoam)
        error("simpleFoam is steady-state: use icoFoam for transient runs");
    if (time.adjustTimeStep && solver == SolverApp::SimpleFoam)
        warning("adjustTimeStep has no effect in steady-state simpleFoam");

    // --- Turbulence ---
    if (turbulence.model == TurbulenceModel::KOmegaSST) {
        if (numerics.ddt != TimeScheme::SteadyState || solver != SolverApp::SimpleFoam)
            error("the k-omega SST model is available for steady simpleFoam cases only");
        if (!std::isfinite(turbulence.intensity) || turbulence.intensity <= 0.0 ||
            turbulence.intensity > 1.0)
            error("turbulence intensity must be greater than 0 and at most 1");
        if (!std::isfinite(turbulence.lengthScale) || turbulence.lengthScale <= 0.0)
            error("turbulent length scale must be positive and finite");
        if (!std::isfinite(turbulence.referenceSpeed) || turbulence.referenceSpeed < 0.0)
            error("turbulence reference speed must be zero (automatic) or positive");
        else if (!(turbulenceReferenceSpeed() > 0.0))
            error("the k-omega SST model needs a prescribed velocity or a reference speed");
    }

    const double re = estimateReynolds();
    if (turbulence.model == TurbulenceModel::Laminar && re > 2000.0)
        warning("estimated Reynolds number is " + std::to_string(re) +
                " (> ~2000): the laminar model may be physically inappropriate");

    return issues;
}

double turbulentKineticEnergy(double speed, double intensity) {
    if (!std::isfinite(speed) || !std::isfinite(intensity) || speed < 0.0 || intensity < 0.0)
        return std::numeric_limits<double>::quiet_NaN();
    const double fluctuation = speed * intensity;
    return 1.5 * fluctuation * fluctuation;
}

double specificDissipationRate(double k, double lengthScale) {
    if (!std::isfinite(k) || !std::isfinite(lengthScale) || k < 0.0 || lengthScale <= 0.0)
        return std::numeric_limits<double>::quiet_NaN();
    constexpr double cMu = 0.09;
    return std::sqrt(k) / (std::pow(cMu, 0.25) * lengthScale);
}

double CaseSetup::turbulenceReferenceSpeed() const {
    if (turbulence.referenceSpeed > 0.0)
        return turbulence.referenceSpeed;
    double speed = internalVelocity.norm();
    for (const auto& bc : boundaries)
        if (bc.velocity == VelocityBc::FixedValue)
            speed = std::max(speed, bc.velocityValue.norm());
    return std::isfinite(speed) ? speed : 0.0;
}

} // namespace vortia
