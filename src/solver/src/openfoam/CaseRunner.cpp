#include "vortia/solver/openfoam/CaseRunner.hpp"

#include "vortia/core/Logger.hpp"
#include "vortia/solver/openfoam/Detector.hpp"

#include <fstream>

namespace vortia::openfoam {

namespace {

/// Appends raw solver output to the case's run log so nothing is lost even
/// if the GUI disconnects.
class LogFileSink {
public:
    explicit LogFileSink(const std::filesystem::path& path)
        : out_(path, std::ios::app | std::ios::binary) {}

    void write(std::string_view chunk) {
        if (out_)
            out_.write(chunk.data(), static_cast<std::streamsize>(chunk.size()));
    }

private:
    std::ofstream out_;
};

} // namespace

Expected<StepOutcome> CaseRunner::execute(const std::string& label,
                                          const std::string& executable,
                                          const std::vector<std::string>& args,
                                          const std::filesystem::path& caseDir,
                                          const RunCallbacks& callbacks,
                                          CancellationToken* token) {
    auto exePath = Detector::resolveExecutable(executable, environment_);
    if (!exePath)
        return Error::make(ErrorCode::BackendUnavailable,
                           "cannot resolve '" + executable + "' for step '" + label + "'",
                           exePath.error().message);

    ProcessRequest request;
    request.executable = exePath.value();
    request.arguments = args;
    request.workingDirectory = caseDir;
    request.environment = environment_;

    LogFileSink logFile(caseDir / "log.vortia");
    logFile.write("\n===== Vortia step: " + label + " (" + executable + ") =====\n");

    LogParser parser;
    auto onOutput = [&](OutputStream stream, std::string_view chunk) {
        logFile.write(chunk);
        if (stream == OutputStream::StdOut)
            parser.feed(chunk);
        if (callbacks.onOutput)
            callbacks.onOutput(stream, chunk);
        if (callbacks.onProgress)
            callbacks.onProgress(parser.progress());
    };

    log(LogLevel::Info, "Running '" + executable + "' in " + caseDir.string());
    auto result = processes_.run(request, token, onOutput);
    if (!result)
        return result.error();

    StepOutcome outcome;
    outcome.name = label;
    outcome.process = std::move(result.value());
    outcome.diverged = parser.progress().diverged;
    outcome.divergenceReason = parser.progress().divergenceReason;
    return outcome;
}

Expected<StepOutcome> CaseRunner::runMeshing(const std::filesystem::path& caseDir,
                                             const Capabilities& caps,
                                             const RunCallbacks& callbacks,
                                             CancellationToken* token) {
    if (!caps.hasBlockMesh)
        return Error::make(ErrorCode::BackendUnavailable,
                           "blockMesh is not available in this OpenFOAM installation");

    auto mesh = execute("blockMesh", "blockMesh", {}, caseDir, callbacks, token);
    if (!mesh)
        return mesh.error();
    if (!mesh->process.succeeded())
        return mesh; // caller inspects exit code / logs

    if (caps.hasCheckMesh && !mesh->process.cancelled) {
        auto check = execute("checkMesh", "checkMesh", {}, caseDir, callbacks, token);
        if (!check)
            return check.error();
        if (!check->process.succeeded()) {
            log(LogLevel::Warning, "checkMesh reported problems; see log.vortia");
            return check; // quality failure is signalled, not hidden
        }
    }
    return mesh;
}

Expected<StepOutcome> CaseRunner::runSolver(const std::filesystem::path& caseDir,
                                            const CaseSetup& setup,
                                            const Capabilities& caps,
                                            const RunCallbacks& callbacks,
                                            CancellationToken* token) {
    const bool steady = setup.numerics.ddt == TimeScheme::SteadyState;
    const std::string& exe = steady ? caps.laminarSteadySolver : caps.laminarTransientSolver;
    const std::vector<std::string>& args =
        steady ? caps.laminarSteadyArgs : caps.laminarTransientArgs;

    if (exe == "icoFoam" && !caps.hasIcoFoam)
        return Error::make(ErrorCode::BackendUnavailable,
                           "icoFoam is not available in this OpenFOAM installation");
    if (exe == "simpleFoam" && !caps.hasSimpleFoam)
        return Error::make(ErrorCode::BackendUnavailable,
                           "simpleFoam is not available in this OpenFOAM installation");
    if (exe == "foamRun" && !caps.hasFoamRun)
        return Error::make(ErrorCode::BackendUnavailable,
                           "foamRun is not available in this OpenFOAM installation");

    return execute(exe, exe, args, caseDir, callbacks, token);
}

} // namespace vortia::openfoam
