#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/core/Process.hpp"
#include "vortia/solver/openfoam/Installation.hpp"

#include <filesystem>
#include <string>
#include <vector>

namespace vortia::openfoam {

struct EngineSelfTestReport {
    /// One line per completed step, in execution order.
    std::vector<std::string> steps;
};

/// Run a tiny real simulation with `installation`: mesh a lid-driven cavity,
/// check the mesh, solve a few iterations serially, then decompose, solve on
/// `processes` MPI ranks and reconstruct. Proves the engine (tools, shared
/// libraries, MPI launcher and partitioner) actually works on this machine;
/// it says nothing about the physical accuracy of any result.
///
/// `workDir` must be an existing, writable, empty directory.
[[nodiscard]] Expected<EngineSelfTestReport>
runEngineSelfTest(const Installation& installation, IProcessRunner& runner,
                  const std::filesystem::path& workDir, int processes = 2);

} // namespace vortia::openfoam
