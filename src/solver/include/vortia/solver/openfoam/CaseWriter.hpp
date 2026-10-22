#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/physics/Physics.hpp"
#include "vortia/solver/openfoam/Installation.hpp"

#include <filesystem>
#include <vector>

namespace vortia::openfoam {

struct WriteOptions {
    bool overwrite{false}; ///< Allow replacing a case previously written by Vortia.
};

/// Materializes a complete OpenFOAM case directory from a validated
/// CaseSetup. Output is deterministic (golden-file testable).
///
/// Safety rules:
///  - refuses to write into a directory containing a case NOT created by
///    Vortia (no `.vortia-case` marker) unless the directory is empty;
///  - never touches files outside `caseDir`;
///  - the solver configuration is taken from Capabilities, so generated
///    files match the detected distribution.
class CaseWriter {
public:
    /// Marker file identifying a case as generated (and disposable) by Vortia.
    static constexpr std::string_view kMarkerFile = ".vortia-case";

    [[nodiscard]] static Expected<std::vector<std::filesystem::path>>
    write(const std::filesystem::path& caseDir, const CaseSetup& setup,
          const Capabilities& caps, const WriteOptions& options = {});

    // Individual file generators (also used by golden-file unit tests).
    [[nodiscard]] static std::string controlDict(const CaseSetup& setup,
                                                 const Capabilities& caps);
    [[nodiscard]] static std::string fvSchemes(const CaseSetup& setup);
    [[nodiscard]] static std::string fvSolution(const CaseSetup& setup);
    [[nodiscard]] static std::string transportProperties(const CaseSetup& setup,
                                                         const Capabilities& caps);
    [[nodiscard]] static std::string turbulenceProperties();
    [[nodiscard]] static std::string fieldU(const CaseSetup& setup);
    [[nodiscard]] static std::string fieldP(const CaseSetup& setup);
};

} // namespace vortia::openfoam
