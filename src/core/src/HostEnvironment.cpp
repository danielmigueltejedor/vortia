#include "vortia/core/HostEnvironment.hpp"

#include <cstdlib>

namespace vortia {

bool restoreHostVariable(const EnvironmentAccess& env, const std::string& name) {
    if (!env.get(std::string{kLauncherMarker}))
        return false;
    const std::string saved = std::string{kHostValuePrefix} + name;
    if (const auto original = env.get(saved))
        env.set(name, *original);
    else
        env.unset(name);
    return true;
}

EnvironmentAccess processEnvironment() {
    return {
        [](const std::string& name) -> std::optional<std::string> {
            const char* value = std::getenv(name.c_str());
            return value ? std::optional<std::string>{value} : std::nullopt;
        },
        [](const std::string& name, const std::string& value) {
            ::setenv(name.c_str(), value.c_str(), 1);
        },
        [](const std::string& name) { ::unsetenv(name.c_str()); },
    };
}

} // namespace vortia
