#include "vortia/core/Project.hpp"

#include "vortia/core/Logger.hpp"

#include <nlohmann/json.hpp>

#include <chrono>
#include <fstream>
#include <sstream>

namespace vortia {

namespace {

[[nodiscard]] std::string nowIso8601Utc() {
    const std::time_t now = std::time(nullptr);
    std::tm tm{};
#ifdef _WIN32
    gmtime_s(&tm, &now);
#else
    gmtime_r(&now, &tm);
#endif
    char buffer[32];
    std::strftime(buffer, sizeof(buffer), "%Y-%m-%dT%H:%M:%SZ", &tm);
    return buffer;
}

[[nodiscard]] Expected<nlohmann::json> readJsonFile(const std::filesystem::path& path) {
    std::ifstream in(path);
    if (!in)
        return Error::make(ErrorCode::IoError, "cannot open manifest for reading",
                           path.string());
    nlohmann::json doc;
    try {
        in >> doc;
    } catch (const nlohmann::json::exception& e) {
        return Error::make(ErrorCode::ParseError, e.what(), path.string());
    }
    return doc;
}

/// Atomic write: temp file in the same directory + rename.
[[nodiscard]] Expected<void> writeFileAtomic(const std::filesystem::path& path,
                                             const std::string& contents) {
    const std::filesystem::path tmp = path.parent_path() / (path.filename().string() + ".tmp");
    {
        std::ofstream out(tmp, std::ios::binary | std::ios::trunc);
        if (!out)
            return Error::make(ErrorCode::IoError, "cannot open temp manifest for writing",
                               tmp.string());
        out << contents;
        out.flush();
        if (!out)
            return Error::make(ErrorCode::IoError, "failed writing temp manifest", tmp.string());
    }
    std::error_code ec;
    std::filesystem::rename(tmp, path, ec);
    if (ec) {
        std::filesystem::remove(tmp);
        return Error::make(ErrorCode::IoError, "failed to rename manifest into place",
                           ec.message());
    }
    return {};
}

} // namespace

Expected<Project> Project::create(const std::filesystem::path& rootDir, std::string name) {
    if (name.empty())
        return Error::make(ErrorCode::InvalidArgument, "project name must not be empty");
    std::error_code ec;
    if (std::filesystem::exists(rootDir, ec) && !std::filesystem::is_empty(rootDir, ec))
        return Error::make(ErrorCode::InvalidArgument,
                           "project directory already exists and is not empty",
                           rootDir.string());
    if (!std::filesystem::create_directories(rootDir, ec) && ec)
        return Error::make(ErrorCode::IoError, "cannot create project directory", ec.message());

    Project project;
    project.rootDir_ = std::filesystem::absolute(rootDir, ec);
    if (ec)
        return Error::make(ErrorCode::IoError, "cannot resolve project path", ec.message());
    project.name_ = std::move(name);
    project.createdUtc_ = nowIso8601Utc();
    project.modifiedUtc_ = project.createdUtc_;

    if (auto saved = project.save(); !saved)
        return saved.error();
    log(LogLevel::Info, "Created project '" + project.name_ + "' at " + project.rootDir_.string());
    return project;
}

Expected<Project> Project::open(const std::filesystem::path& rootDir) {
    const std::filesystem::path manifest = rootDir / std::string(kManifestFileName);
    if (!std::filesystem::is_regular_file(manifest))
        return Error::make(ErrorCode::NotFound, "not a Vortia project (manifest missing)",
                           manifest.string());

    auto doc = readJsonFile(manifest);
    if (!doc)
        return doc.error();

    try {
        const auto& j = doc.value();
        const int schema = j.value("schemaVersion", -1);
        if (schema != kSchemaVersion)
            return Error::make(ErrorCode::UnsupportedVersion,
                               "unsupported project schemaVersion " + std::to_string(schema),
                               manifest.string());

        Project project;
        std::error_code ec;
        project.rootDir_ = std::filesystem::absolute(rootDir, ec);
        if (ec)
            return Error::make(ErrorCode::IoError, "cannot resolve project path", ec.message());
        project.name_ = j.value("name", std::string{});
        if (project.name_.empty())
            return Error::make(ErrorCode::InvalidData, "project manifest has no name",
                               manifest.string());
        project.createdUtc_ = j.value("created", std::string{});
        project.modifiedUtc_ = j.value("modified", std::string{});
        project.backendFingerprint_ = j.value("backendFingerprint", std::string{});
        if (j.contains("setup"))
            project.setupJson_ = j["setup"].dump();
        const auto state = caseStateFromString(j.value("caseState", std::string{"notCreated"}));
        if (!state)
            return state.error();
        project.caseState_ = state.value();
        return project;
    } catch (const nlohmann::json::exception& e) {
        return Error::make(ErrorCode::InvalidData, e.what(), manifest.string());
    }
}

Expected<void> Project::save() {
    if (rootDir_.empty())
        return Error::make(ErrorCode::InvalidArgument, "project has no root directory");
    modifiedUtc_ = nowIso8601Utc();

    nlohmann::json setup;
    try {
        setup = nlohmann::json::parse(setupJson_);
    } catch (const nlohmann::json::exception& e) {
        return Error::make(ErrorCode::InvalidData,
                           std::string("setup payload is not valid JSON: ") + e.what());
    }

    nlohmann::json j;
    j["schemaVersion"] = kSchemaVersion;
    j["name"] = name_;
    j["created"] = createdUtc_;
    j["modified"] = modifiedUtc_;
    j["backendFingerprint"] = backendFingerprint_;
    j["caseState"] = std::string(toString(caseState_));
    j["setup"] = setup;

    return writeFileAtomic(manifestPath(), j.dump(2) + '\n');
}

void Project::setName(std::string name) {
    if (!name.empty())
        name_ = std::move(name);
}

void Project::setSetupJson(std::string json) {
    setupJson_ = std::move(json);
}

void Project::setBackendFingerprint(std::string fingerprint) {
    backendFingerprint_ = std::move(fingerprint);
}

void Project::setCaseState(CaseState state) {
    caseState_ = state;
}

std::string_view Project::toString(CaseState state) {
    switch (state) {
    case CaseState::NotCreated: return "notCreated";
    case CaseState::Configured: return "configured";
    case CaseState::Meshed: return "meshed";
    case CaseState::Solved: return "solved";
    }
    return "notCreated";
}

Expected<Project::CaseState> Project::caseStateFromString(std::string_view text) {
    if (text == "notCreated")
        return CaseState::NotCreated;
    if (text == "configured")
        return CaseState::Configured;
    if (text == "meshed")
        return CaseState::Meshed;
    if (text == "solved")
        return CaseState::Solved;
    return Error::make(ErrorCode::InvalidData, "unknown caseState '" + std::string(text) + "'");
}

} // namespace vortia
