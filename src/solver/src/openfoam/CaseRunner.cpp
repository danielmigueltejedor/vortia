#include "vortia/solver/openfoam/CaseRunner.hpp"

#include "vortia/core/Logger.hpp"
#include "vortia/solver/openfoam/Detector.hpp"

#include <fstream>
#include <cmath>

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
    try {
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
    parser.flush();
    if (callbacks.onProgress)
        callbacks.onProgress(parser.progress());
    outcome.name = label;
    outcome.process = std::move(result.value());
    outcome.diverged = parser.progress().diverged;
    outcome.divergenceReason = parser.progress().divergenceReason;
    outcome.finalTime = parser.progress().time;
    return outcome;
    } catch (const std::exception& e) {
        return Error::make(ErrorCode::Internal, std::string("exception in ") + label + ": " + e.what());
    } catch (...) {
        return Error::make(ErrorCode::Internal, "unknown exception in " + label);
    }
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
    // Only the dedicated Vortia nozzle preparer opts into initialization.
    // Resuming an existing solution must never reset its fields.
    std::ifstream marker(caseDir / ".vortia-nozzle-case");
    std::string kind;
    std::getline(marker, kind);
    if (kind == "vortia-nozzle-video-2026-v1" || kind == "vortia-nozzle-video-2026-v2"
        || kind == "vortia-nozzle-video-2026-v3") {
        bool hasSolution = false;
        for (const auto& entry : std::filesystem::directory_iterator(caseDir)) {
            if (!entry.is_directory()) continue;
            const auto name = entry.path().filename().string();
            try {
                std::size_t used = 0;
                const double time = std::stod(name, &used);
                if (used == name.size() && std::isfinite(time) && time > 0) hasSolution = true;
            } catch (...) {} // ordinary folders such as constant/system
        }
        if (!hasSolution)
            return execute("initialize nozzle", "setFields", {}, caseDir, callbacks, token);
    }
    return mesh;
}

Expected<StepOutcome> CaseRunner::runSolver(const std::filesystem::path& caseDir,
                                            const CaseSetup& setup,
                                            const Capabilities& caps,
                                            const RunCallbacks& callbacks,
                                            CancellationToken* token,
                                            const std::string& solverOverride) {
    std::string exe;
    std::vector<std::string> args;
    if (!solverOverride.empty()) {
        exe = solverOverride;
    } else {
        const bool steady = setup.numerics.ddt == TimeScheme::SteadyState;
        exe = steady ? caps.laminarSteadySolver : caps.laminarTransientSolver;
        args = steady ? caps.laminarSteadyArgs : caps.laminarTransientArgs;
        if (exe == "icoFoam" && !caps.hasIcoFoam)
            return Error::make(ErrorCode::BackendUnavailable,
                               "icoFoam is not available in this OpenFOAM installation");
        if (exe == "simpleFoam" && !caps.hasSimpleFoam)
            return Error::make(ErrorCode::BackendUnavailable,
                               "simpleFoam is not available in this OpenFOAM installation");
        if (exe == "foamRun" && !caps.hasFoamRun)
            return Error::make(ErrorCode::BackendUnavailable,
                               "foamRun is not available in this OpenFOAM installation");
    }
    if (exe == "vortiaCentralFoam") {
        std::ifstream marker(caseDir / ".vortia-nozzle-case");
        std::string kind;
        std::getline(marker,kind);
        if (kind != "vortia-nozzle-video-2026-v3")
            return Error::make(ErrorCode::InvalidData,
                "prepare a fresh v3 no-swirl nozzle case; earlier or unmarked cases cannot use this backend");
        if (kind == "vortia-nozzle-video-2026-v3") {
            std::string version;
            for (const auto& [key,value] : environment_)
                if (key=="WM_PROJECT_VERSION") version=value;
            if (version!="v2606" && version!="2606")
                return Error::make(ErrorCode::BackendUnavailable,
                    "the experimental nozzle backend requires OpenFOAM v2606");
            const auto bin=std::filesystem::absolute(caseDir/"bin");
            if (!std::filesystem::is_regular_file(bin/exe)) {
                if (!std::filesystem::is_regular_file(caseDir/"solver"/"vortiaCentralFoam.C"))
                    return Error::make(ErrorCode::InvalidData,"nozzle solver sources are missing");
                std::filesystem::create_directories(bin);
                auto buildEnvironment=environment_;
                bool outputSet=false;
                for (auto& [key,value] : buildEnvironment)
                    if (key=="VORTIA_NOZZLE_OUTPUT") { value=(bin/exe).string(); outputSet=true; }
                if (!outputSet) buildEnvironment.emplace_back("VORTIA_NOZZLE_OUTPUT",(bin/exe).string());
                CaseRunner builder(processes_,std::move(buildEnvironment));
                auto built=builder.execute("build nozzle backend","wmake",{"solver"},caseDir,callbacks,token);
                if (!built || !built->process.succeeded()) return built;
            }
            // Scope the case binary to this dedicated solver invocation. The
            // general executable resolver still accepts plain names only.
            auto environment=environment_;
            for (auto& [key,value] : environment)
                if (key=="PATH") value=bin.string()+":"+value;
            CaseRunner solver(processes_,std::move(environment));
            return solver.execute(exe,exe,args,caseDir,callbacks,token);
        }
    }
    return execute(exe, exe, args, caseDir, callbacks, token);
}

} // namespace vortia::openfoam
