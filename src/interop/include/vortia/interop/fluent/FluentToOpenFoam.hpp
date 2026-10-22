#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/interop/fluent/FluentTypes.hpp"

#include <filesystem>
#include <vector>

namespace vortia::fluent {

/// Converts a FluentMesh into an OpenFOAM case directory. Writes the mesh
/// under `caseDir/constant/polyMesh/` (points, faces, owner, neighbour,
/// boundary) plus a minimal set of OpenFOAM control files so that
/// `checkMesh` can validate it. Returns the list of written files.
///
/// Limitations (documented in docs/FLUENT_CONVERSION.md):
///  - Only tetra, hexa, pyramid and wedge cells are supported; polyhedra
///    and mixed cells with unsupported topology are reported.
///  - Boundary condition *values* are not transferred (BC types are mapped
///    to patch types; values must be set by the user).
///  - No turbulence/physics model is inferred.
[[nodiscard]] Expected<std::vector<std::filesystem::path>>
convertToOpenFoam(const FluentMesh& mesh, const std::filesystem::path& caseDir,
                  CompatibilityReport* report = nullptr);

/// Builds a CompatibilityReport for a Fluent mesh (no files written).
[[nodiscard]] CompatibilityReport assessCompatibility(const FluentMesh& mesh,
                                                      FluentFormat format,
                                                      const std::string& filePath);

} // namespace vortia::fluent
