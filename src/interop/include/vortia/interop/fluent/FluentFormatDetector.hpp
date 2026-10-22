#pragma once

#include "vortia/interop/fluent/FluentTypes.hpp"

#include <filesystem>

namespace vortia::fluent {

/// Identifies a Fluent file by extension AND by content (magic bytes /
/// header). Returns Unknown when neither matches.
[[nodiscard]] FluentFormat detectFormat(const std::filesystem::path& path);

/// True when the file begins with the HDF5 magic number.
[[nodiscard]] bool isHdf5(const std::filesystem::path& path);

/// True when the file is gzip-compressed.
[[nodiscard]] bool isGzip(const std::filesystem::path& path);

/// Reads the first few hundred bytes as text for header-based detection.
[[nodiscard]] std::string readHeader(const std::filesystem::path& path, std::size_t maxBytes = 4096);

} // namespace vortia::fluent
