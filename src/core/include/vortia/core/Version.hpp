#pragma once

#include "vortia/core/Expected.hpp"

#include <compare>
#include <string>
#include <string_view>

namespace vortia {

/// Semantic-ish version tolerant to vendor schemes ("v2606", "14", "9.4.2").
/// Keeps the raw string because OpenFOAM distributions encode the
/// distribution family in the version format itself.
struct Version {
    int major{0};
    int minor{0};
    int patch{0};
    std::string raw;

    auto operator<=>(const Version&) const = default;

    /// Parses "v2412", "2412", "14", "9.4.2", "10.0"… leading 'v' is kept in raw.
    [[nodiscard]] static Expected<Version> parse(std::string_view text);
};

} // namespace vortia
