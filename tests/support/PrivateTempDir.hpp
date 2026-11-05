#pragma once

#include <chrono>
#include <cstdlib>
#include <filesystem>
#include <stdexcept>
#include <string>
#include <string_view>

#ifndef _WIN32
#include <unistd.h>
#endif

namespace vortia::test {

/// Creates an exclusive temporary directory with owner-only permissions.
/// Prefer this over `temp_directory_path() / predictable-name`, which is a
/// TOCTOU race in a publicly writable directory (Sonar cpp:S5443).
inline std::filesystem::path makePrivateTempDir(std::string_view prefix) {
#ifdef _WIN32
    auto path = std::filesystem::temp_directory_path() /
                (std::string(prefix) + "-" +
                 std::to_string(
                     std::chrono::steady_clock::now().time_since_epoch().count()));
    std::filesystem::create_directories(path);
#else
    const char* tmp = std::getenv("TMPDIR");
    if (tmp == nullptr || tmp[0] == '\0')
        tmp = "/tmp";
    std::string tmpl = std::string(tmp) + "/" + std::string(prefix) + "XXXXXX";
    if (mkdtemp(tmpl.data()) == nullptr)
        throw std::runtime_error("failed to create private temporary directory");
    auto path = std::filesystem::path(tmpl);
#endif
    std::filesystem::permissions(path, std::filesystem::perms::owner_all,
                                 std::filesystem::perm_options::replace);
    return path;
}

} // namespace vortia::test
