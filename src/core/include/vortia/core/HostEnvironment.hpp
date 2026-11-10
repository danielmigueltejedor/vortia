#pragma once

// Undo the packaging launcher's environment for everything Vortia starts.
//
// The Linux AppImage launcher (packaging/linux/AppRun) points the dynamic
// loader and Qt at the libraries bundled inside the image. Those settings
// must not reach child processes: OpenFOAM solvers, python3 or gmsh from the
// host would otherwise load Vortia's bundled libssl/libstdc++/Qt instead of
// their own. The launcher therefore records the user's original values and
// Vortia restores them once its own libraries are loaded.

#include <functional>
#include <optional>
#include <string>
#include <string_view>

namespace vortia {

struct EnvironmentAccess {
    std::function<std::optional<std::string>(const std::string&)> get;
    std::function<void(const std::string&, const std::string&)> set;
    std::function<void(const std::string&)> unset;
};

/// Marker exported by the launcher; restoration is a no-op without it.
inline constexpr std::string_view kLauncherMarker = "VORTIA_LAUNCHER_ENV";
/// Prefix of the variables holding the user's original values.
inline constexpr std::string_view kHostValuePrefix = "VORTIA_HOST_";

/// Restore `name` to the value the user had before the launcher ran: the
/// saved value when one was recorded, otherwise unset. Returns false (and
/// changes nothing) when Vortia was not started through the launcher.
bool restoreHostVariable(const EnvironmentAccess& env, const std::string& name);

/// Process environment (getenv/setenv/unsetenv).
EnvironmentAccess processEnvironment();

} // namespace vortia
