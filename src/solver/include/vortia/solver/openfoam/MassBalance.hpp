#pragma once

#include <filesystem>
#include <string>

namespace vortia::openfoam {

/// Result of a nozzle mass-balance check.
struct MassBalanceResult {
    bool available{false};
    int firstIteration{0};
    int lastIteration{0};
    double meanAbsolutePercent{0.0};
    double maximumAbsolutePercent{0.0};
    int samples{0};
};

/// Returns true when the case directory looks like a nozzle-practice case.
[[nodiscard]] bool isNozzlePracticeCase(const std::filesystem::path& caseDir);

/// Reads mass-balance data from an OpenFOAM case directory.
/// Returns a result with available=false when no usable data is found.
[[nodiscard]] MassBalanceResult nozzleMassBalance(const std::filesystem::path& caseDir);

} // namespace vortia::openfoam
