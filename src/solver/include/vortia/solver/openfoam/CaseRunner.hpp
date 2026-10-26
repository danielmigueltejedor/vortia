#pragma once

#include "vortia/core/Process.hpp"
#include "vortia/physics/Physics.hpp"
#include "vortia/solver/openfoam/Installation.hpp"
#include "vortia/solver/openfoam/LogParser.hpp"

#include <filesystem>
#include <functional>

namespace vortia::openfoam {

/// Outcome of one pipeline step.
struct StepOutcome {
    std::string name;
    ProcessResult process;
    bool diverged{false};
    DivergenceKind divergenceKind{DivergenceKind::None};
    std::string divergenceReason;
    double finalTime{0.0}; ///< Last "Time = " parsed from the solver log.
};

/// Progress sink for GUI/console wiring.
struct RunCallbacks {
    OutputCallback onOutput;                                  ///< Raw log chunks.
    std::function<void(const SolverProgress&)> onProgress;    ///< Parsed state.
    std::function<void(const std::vector<MonitorSample>&)> onMonitors; ///< Every parsed observation; caller batches GUI delivery.
};

/// Runs the OpenFOAM pipeline (blockMesh → solver) inside a case directory
/// using a captured OpenFOAM environment. All launches are structured
/// ProcessRequests resolved against the captured PATH — never a shell.
///
/// The runner is synchronous; callers (GUI) execute it on a worker thread.
class CaseRunner {
public:
    CaseRunner(IProcessRunner& processes,
               std::vector<std::pair<std::string, std::string>> environment)
        : processes_(processes), environment_(std::move(environment)) {}

    /// Runs blockMesh (and checkMesh when available) in `caseDir`.
    [[nodiscard]] Expected<StepOutcome> runMeshing(const std::filesystem::path& caseDir,
                                                   const Capabilities& caps,
                                                   const RunCallbacks& callbacks,
                                                   CancellationToken* token);

    /// Runs the laminar solver selected by the setup (transient/steady).
    [[nodiscard]] Expected<StepOutcome> runSolver(const std::filesystem::path& caseDir,
                                                  const CaseSetup& setup,
                                                  const Capabilities& caps,
                                                  const RunCallbacks& callbacks,
                                                  CancellationToken* token,
                                                  const std::string& solverOverride = {},
                                                  unsigned processes = 1);

private:
    [[nodiscard]] Expected<StepOutcome> runParallelFlow(const std::filesystem::path& caseDir,
        unsigned processes, const RunCallbacks&, CancellationToken*);
    [[nodiscard]] Expected<StepOutcome> execute(const std::string& label,
                                                const std::string& executable,
                                                const std::vector<std::string>& args,
                                                const std::filesystem::path& caseDir,
                                                const RunCallbacks& callbacks,
                                                CancellationToken* token);

    IProcessRunner& processes_;
    std::vector<std::pair<std::string, std::string>> environment_;
};

} // namespace vortia::openfoam
