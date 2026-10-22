#pragma once

#include "vortia/interop/fluent/FluentTypes.hpp"

#include "vortia/core/Expected.hpp"

#include <filesystem>

namespace vortia::fluent {

/// Reads a traditional Fluent .msh (ASCII) file into the intermediate
/// FluentMesh model. Returns an error if the file is not a recognisable
/// Fluent mesh or is structurally invalid.
[[nodiscard]] Expected<FluentMesh> readTraditionalMesh(const std::filesystem::path& path);

/// Convenience: read any Fluent mesh file, dispatching by detected format.
[[nodiscard]] Expected<FluentMesh> readMesh(const std::filesystem::path& path);

} // namespace vortia::fluent
