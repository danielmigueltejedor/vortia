#pragma once

#include <chrono>
#include <filesystem>
#include <stdexcept>
#include <string>
#include <string_view>

#ifndef _WIN32
#include <unistd.h>
#endif

namespace vortia::test {

/// Creates an exclusive temporary directory with owner-only permissions under
/// the process working directory (the CTest build tree), never under a
/// world-writable system temp path (Sonar cpp:S5443).
inline std::filesystem::path makePrivateTempDir(std::string_view prefix) {
    const auto base = std::filesystem::current_path() / ".vortia-test-tmp";
    std::error_code ec;
    std::filesystem::create_directories(base, ec);
    if (ec)
        throw std::runtime_error("failed to create private temporary base directory");

#ifdef _WIN32
    auto path = base /
                (std::string(prefix) + "-" +
                 std::to_string(
                     std::chrono::steady_clock::now().time_since_epoch().count()));
    if (!std::filesystem::create_directory(path))
        throw std::runtime_error("failed to create private temporary directory");
#else
    std::string tmpl = (base / (std::string(prefix) + "XXXXXX")).string();
    if (mkdtemp(tmpl.data()) == nullptr)
        throw std::runtime_error("failed to create private temporary directory");
    auto path = std::filesystem::path(tmpl);
#endif
    std::filesystem::permissions(path, std::filesystem::perms::owner_all,
                                 std::filesystem::perm_options::replace);
    return path;
}

} // namespace vortia::test
