#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/geometry/TriangleMesh.hpp"

#include <filesystem>

namespace vortia {

/// Reads STL files in both encodings.
///
/// Format detection: binary STL has an exact size of 84 + 50·nbytes; when the
/// size matches we trust binary even if the header starts with "solid" (a
/// common exporter quirk). Otherwise the ASCII grammar is parsed strictly.
///
/// Limits: files larger than kMaxFileSize are refused (protects the GUI from
/// swapping on accidental multi-GB picks); per-vertex coordinates must be
/// finite numbers.
class StlReader {
public:
    static constexpr std::uintmax_t kMaxFileSize = 2ull * 1024 * 1024 * 1024; // 2 GiB

    [[nodiscard]] static Expected<TriangleMesh> read(const std::filesystem::path& path);
};

} // namespace vortia
