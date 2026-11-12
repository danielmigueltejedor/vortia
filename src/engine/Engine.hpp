#pragma once

#include "vortia/core/Process.hpp"
#include "vortia/core/Project.hpp"
#include "vortia/solver/openfoam/Installation.hpp"

#include <nlohmann/json.hpp>

#include <atomic>
#include <filesystem>
#include <functional>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <thread>
#include <utility>
#include <vector>

class vtkDataObject;

namespace vortia::engine {

/// The protocol version a client checks in `hello`. Bump on any incompatible change.
inline constexpr int kProtocolVersion = 1;

/// Vortia's case pipeline without an interface: projects, case setup, meshing, solving and
/// result queries, driven by newline-delimited JSON (docs/architecture/native-engine.md).
///
/// One request at a time arrives from `handle`; meshing and solving run on a worker thread and
/// report through `emit`, so a client stays responsive and can cancel.
class Engine {
public:
    using Emit = std::function<void(const nlohmann::json&)>;

    explicit Engine(Emit sink, std::filesystem::path bundledOpenFoam = {});
    ~Engine();
    Engine(const Engine&) = delete;
    Engine& operator=(const Engine&) = delete;

    /// Answers one request: `{"id", "ok", "result" | "error"}`.
    [[nodiscard]] nlohmann::json handle(const nlohmann::json& request);

    /// Waits for a running job, for an orderly exit.
    void join();

private:
    nlohmann::json hello();
    nlohmann::json detect();
    nlohmann::json createProject(const nlohmann::json& request);
    nlohmann::json openProject(const nlohmann::json& request);
    nlohmann::json save(const nlohmann::json& request);
    nlohmann::json validate(const nlohmann::json& request);
    nlohmann::json startJob(const nlohmann::json& request, bool solve);
    nlohmann::json cancel();
    nlohmann::json results();
    nlohmann::json statistics(const nlohmann::json& request);
    nlohmann::json line(const nlohmann::json& request);
    nlohmann::json slice(const nlohmann::json& request);
    nlohmann::json surface(const nlohmann::json& request);
    nlohmann::json section(const nlohmann::json& request);

    nlohmann::json describeProject() const;
    nlohmann::json describeOpenFoam() const;
    void requireProject() const;
    void requireIdle() const;
    bool ensureEnvironment(std::string& error);
    void runJob(bool solve, unsigned processes);
    vtkDataObject* loadResults();

    Emit emit_;
    std::filesystem::path bundledOpenFoam_;
    std::optional<Project> project_;

    std::optional<openfoam::Installation> installation_;
    std::vector<std::pair<std::string, std::string>> environment_;
    openfoam::Capabilities caps_;
    std::string detectionError_;

    std::thread worker_;
    std::atomic<bool> running_{false};
    std::unique_ptr<CancellationToken> cancel_;
    /// Guards the project while a job updates its state from the worker.
    mutable std::mutex projectMutex_;

    struct Results;
    std::unique_ptr<Results> results_;
};

} // namespace vortia::engine
