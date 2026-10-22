#pragma once

#include "vortia/core/Version.hpp"

#include <filesystem>
#include <string>
#include <vector>

namespace vortia::openfoam {

/// OpenFOAM lineage. The two distributions diverge in solver names,
/// dictionary conventions and module layout — never assume interchangeability.
enum class Distribution { Unknown, Esi, Foundation };

[[nodiscard]] std::string_view toString(Distribution d);

/// A located OpenFOAM installation (static facts only — no process was run
/// to obtain them, so detection is testable with fixture directory trees).
struct Installation {
    Distribution distribution{Distribution::Unknown};
    Version version{};
    std::filesystem::path rootDir; ///< WM_PROJECT_DIR
    std::filesystem::path bashrc;  ///< <root>/etc/bashrc

    /// Stable identity string recorded in projects ("esi/v2606@/opt/OpenFOAM-v2606").
    [[nodiscard]] std::string fingerprint() const;
};

/// What a given (distribution, version) can do, per the compatibility table
/// in docs/architecture/research.md. `verifiedOnSystem` is false until the
/// executables were actually found on the target machine.
struct Capabilities {
    bool hasBlockMesh{false};
    bool hasSnappyHexMesh{false};
    bool hasCheckMesh{false};
    bool hasFoamToVtk{false};
    bool hasIcoFoam{false};
    bool hasSimpleFoam{false};
    bool hasFoamRun{false};
    bool verifiedOnSystem{false};

    /// Name of the transport dictionary: "transportProperties" (ESI,
    /// Foundation ≤ 11) or "physicalProperties" (Foundation ≥ 12).
    std::string transportDict{"transportProperties"};

    /// Command for a transient laminar incompressible run.
    std::string laminarTransientSolver{"icoFoam"};
    std::vector<std::string> laminarTransientArgs{};

    /// Command for a steady laminar incompressible run.
    std::string laminarSteadySolver{"simpleFoam"};
    std::vector<std::string> laminarSteadyArgs{};

    /// True when entries above come from the static table but have NOT yet
    /// been confirmed against this exact installation.
    [[nodiscard]] bool needsVerification() const { return !verifiedOnSystem; }
};

/// Static compatibility table. Sources: docs/architecture/research.md §1.
/// Values for Foundation ≥ 10 rely on `foamRun -solver incompressibleFluid`
/// and MUST be verified on a real installation before production use.
[[nodiscard]] Capabilities capabilitiesFor(const Installation& installation);

} // namespace vortia::openfoam
