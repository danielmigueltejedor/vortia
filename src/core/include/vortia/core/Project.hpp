#pragma once

#include "vortia/core/Expected.hpp"

#include <filesystem>
#include <string>

namespace vortia {

/// A Vortia project is a directory containing a versioned JSON manifest
/// (`project.vortia`) plus working data (e.g. the OpenFOAM case directory).
///
/// Guarantees:
/// - atomic manifest writes (temp file + rename) — a crash never leaves a
///   half-written project;
/// - schema versioning with migration hook;
/// - a project never modifies files outside its root directory.
class Project {
public:
    static constexpr int kSchemaVersion = 1;
    static constexpr std::string_view kManifestFileName = "project.vortia";
    static constexpr std::string_view kCaseDirName = "case";

    /// Creates a new project directory (must not exist or be empty).
    [[nodiscard]] static Expected<Project> create(const std::filesystem::path& rootDir,
                                                  std::string name);

    /// Opens and validates an existing project.
    [[nodiscard]] static Expected<Project> open(const std::filesystem::path& rootDir);

    /// Persists the manifest atomically.
    [[nodiscard]] Expected<void> save();

    [[nodiscard]] const std::filesystem::path& rootDir() const { return rootDir_; }
    [[nodiscard]] std::filesystem::path manifestPath() const {
        return rootDir_ / std::string(kManifestFileName);
    }
    [[nodiscard]] std::filesystem::path caseDir() const {
        return rootDir_ / std::string(kCaseDirName);
    }

    [[nodiscard]] const std::string& name() const { return name_; }
    void setName(std::string name);

    /// Opaque JSON payload managed by upper layers (geometry, mesh, physics…).
    /// Core only guarantees persistence and schema integrity.
    [[nodiscard]] const std::string& setupJson() const { return setupJson_; }
    void setSetupJson(std::string json);

    /// Backend identity recorded when a case was generated ("esi/v2606@/opt/…").
    [[nodiscard]] const std::string& backendFingerprint() const { return backendFingerprint_; }
    void setBackendFingerprint(std::string fingerprint);

    /// Lifecycle of the embedded OpenFOAM case.
    enum class CaseState { NotCreated, Configured, Meshed, Solved };
    [[nodiscard]] CaseState caseState() const { return caseState_; }
    void setCaseState(CaseState state);

    [[nodiscard]] static std::string_view toString(CaseState state);
    [[nodiscard]] static Expected<CaseState> caseStateFromString(std::string_view text);

private:
    Project() = default;

    std::filesystem::path rootDir_;
    std::string name_;
    std::string createdUtc_;
    std::string modifiedUtc_;
    std::string backendFingerprint_;
    std::string setupJson_{"{}"};
    CaseState caseState_{CaseState::NotCreated};
};

} // namespace vortia
