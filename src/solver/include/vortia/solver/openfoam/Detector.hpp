#pragma once

#include "vortia/core/Process.hpp"
#include "vortia/solver/openfoam/Installation.hpp"

#include <filesystem>
#include <vector>

namespace vortia::openfoam {

/// Locates and classifies OpenFOAM installations.
///
/// Two clearly separated phases:
///  1. probe()/scan(): pure filesystem + text inspection. No process is
///     spawned, which keeps detection unit-testable with fixture trees.
///  2. captureEnvironment(): sources the installation's etc/bashrc in a
///     clean bash and dumps the resulting environment. This is the ONLY
///     shell invocation in Vortia; the path is validated beforehand and the
///     command line contains no user data beyond that validated path.
class Detector {
public:
    /// Inspects a candidate WM_PROJECT_DIR. Fails if it does not look like
    /// an OpenFOAM root (missing etc/bashrc or unparseable version).
    [[nodiscard]] static Expected<Installation> probe(const std::filesystem::path& rootDir);

    /// Searches standard locations + WM_PROJECT_DIR / VORTIA_OPENFOAM_DIR.
    [[nodiscard]] static std::vector<Installation> scan();

    /// Sources etc/bashrc in `bash --noprofile --norc` and captures the
    /// environment for later structured (shell-free) process launches.
    [[nodiscard]] static Expected<std::vector<std::pair<std::string, std::string>>>
    captureEnvironment(const Installation& installation, IProcessRunner& runner);

    /// Verifies which executables are really reachable given the captured
    /// environment's PATH, and marks caps.verifiedOnSystem accordingly.
    [[nodiscard]] static Capabilities
    verifyCapabilities(const Installation& installation,
                       const std::vector<std::pair<std::string, std::string>>& environment);

    /// Resolves an executable name against a PATH list (no shell involved).
    [[nodiscard]] static Expected<std::filesystem::path>
    resolveExecutable(const std::string& name,
                      const std::vector<std::pair<std::string, std::string>>& environment);
};

} // namespace vortia::openfoam
