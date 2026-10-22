#include "vortia/solver/openfoam/Installation.hpp"

namespace vortia::openfoam {

std::string_view toString(Distribution d) {
    switch (d) {
    case Distribution::Esi: return "esi";
    case Distribution::Foundation: return "foundation";
    case Distribution::Unknown: return "unknown";
    }
    return "unknown";
}

std::string Installation::fingerprint() const {
    return std::string(toString(distribution)) + "/" + version.raw + "@" + rootDir.string();
}

Capabilities capabilitiesFor(const Installation& installation) {
    Capabilities caps;

    // Common utilities: present in every supported release of both
    // distributions (blockMesh and checkMesh are ancient; foamToVTK ≥ OF1.7).
    caps.hasBlockMesh = true;
    caps.hasCheckMesh = true;
    caps.hasFoamToVtk = true;
    caps.hasSnappyHexMesh = true;

    if (installation.distribution == Distribution::Esi) {
        // ESI retains the classic application solvers (research.md, D1).
        caps.hasIcoFoam = true;
        caps.hasSimpleFoam = true;
        caps.hasFoamRun = true; // foamRun also exists in ESI
        caps.transportDict = "transportProperties";
        caps.laminarTransientSolver = "icoFoam";
        caps.laminarSteadySolver = "simpleFoam";
    } else if (installation.distribution == Distribution::Foundation) {
        // Foundation removed application solvers in favour of modules run via
        // foamRun (icoFoam removed in v10; simpleFoam in v11).
        const int major = installation.version.major;
        if (major >= 10) {
            caps.hasFoamRun = true;
            caps.hasIcoFoam = false;
            caps.hasSimpleFoam = false;
            caps.laminarTransientSolver = "foamRun";
            caps.laminarTransientArgs = {"-solver", "incompressibleFluid"};
            // Steady-state via module: requires steadyState ddt scheme in
            // fvSchemes (handled by CaseWriter). PENDING: verify on install.
            caps.laminarSteadySolver = "foamRun";
            caps.laminarSteadyArgs = {"-solver", "incompressibleFluid"};
        } else {
            caps.hasIcoFoam = true;
            caps.hasSimpleFoam = true;
            caps.hasFoamRun = false;
            caps.laminarTransientSolver = "icoFoam";
            caps.laminarSteadySolver = "simpleFoam";
        }
        caps.transportDict = (major >= 12) ? "physicalProperties" : "transportProperties";
    }
    // Distribution::Unknown keeps conservative defaults (ESI-style names),
    // but callers must treat needsVerification() as a hard stop.

    return caps;
}

} // namespace vortia::openfoam
