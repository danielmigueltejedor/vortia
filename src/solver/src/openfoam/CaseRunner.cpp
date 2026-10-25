#include "vortia/solver/openfoam/CaseRunner.hpp"

#include "vortia/core/Logger.hpp"
#include "vortia/solver/openfoam/Detector.hpp"

#include <fstream>
#include <cmath>
#include <chrono>
#include <algorithm>

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
    std::ifstream flowMarker(caseDir/".vortia-flow-case");
    std::string flowKind;
    std::getline(flowMarker,flowKind);
    if (flowKind=="vortia-axisymmetric-flow-v1") {
        if (!std::filesystem::is_regular_file(caseDir/"constant/polyMesh/boundary"))
            return Error::make(ErrorCode::InvalidData,"The CAD mesh is missing; generate a new mesh revision");
        return execute("check CAD mesh","checkMesh",{},caseDir,callbacks,token);
    }
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
                                            const std::string& solverOverride,
                                            unsigned processes) {
    if (processes<1 || processes>8 || (processes>1 && solverOverride!="vortiaCentralFoam"))
        return Error::make(ErrorCode::InvalidArgument,"Select 1 to 8 processes for the generated compressible backend");
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
        std::ifstream flowMarker(caseDir/".vortia-flow-case");
        std::string flowKind;
        std::getline(flowMarker,flowKind);
        const bool generalFlow=flowKind=="vortia-axisymmetric-flow-v1";
        if (kind != "vortia-nozzle-video-2026-v3" && !generalFlow)
            return Error::make(ErrorCode::InvalidData,
                "prepare a fresh v3 no-swirl nozzle case; earlier or unmarked cases cannot use this backend");
        if (kind == "vortia-nozzle-video-2026-v3" || generalFlow) {
            std::string version;
            for (const auto& [key,value] : environment_)
                if (key=="WM_PROJECT_VERSION") version=value;
            if (version!="v2606" && version!="2606")
                return Error::make(ErrorCode::BackendUnavailable,
                    "the experimental nozzle backend requires OpenFOAM v2606");
            std::filesystem::path source="solver", binRelative="bin";
            std::ifstream revisionFile(caseDir/".vortia-solver-revision");
            std::string revision;
            if (std::getline(revisionFile,revision)) {
                if (revision.size()!=64 || !std::all_of(revision.begin(),revision.end(),[](char c) {
                    return (c>='0'&&c<='9') || (c>='a'&&c<='f');
                })) return Error::make(ErrorCode::InvalidData,"Invalid bundled solver revision");
                source=std::filesystem::path("solver-backends")/revision;
                binRelative/=revision;
            } else if (processes>1) {
                return Error::make(ErrorCode::InvalidData,"Parallel flow requires the current bundled solver revision");
            }
            const auto bin=std::filesystem::absolute(caseDir/binRelative);
            if (!std::filesystem::is_regular_file(bin/exe)) {
                if (!std::filesystem::is_regular_file(caseDir/source/"vortiaCentralFoam.C"))
                    return Error::make(ErrorCode::InvalidData,"nozzle solver sources are missing");
                std::filesystem::create_directories(bin);
                auto buildEnvironment=environment_;
                bool outputSet=false;
                for (auto& [key,value] : buildEnvironment)
                    if (key=="VORTIA_NOZZLE_OUTPUT") { value=(bin/exe).string(); outputSet=true; }
                if (!outputSet) buildEnvironment.emplace_back("VORTIA_NOZZLE_OUTPUT",(bin/exe).string());
                CaseRunner builder(processes_,std::move(buildEnvironment));
                auto built=builder.execute("build nozzle backend","wmake",{source.string()},caseDir,callbacks,token);
                if (!built || !built->process.succeeded()) return built;
            }
            // Scope the case binary to this dedicated solver invocation. The
            // general executable resolver still accepts plain names only.
            auto environment=environment_;
            for (auto& [key,value] : environment)
                if (key=="PATH") value=bin.string()+":"+value;
            CaseRunner solver(processes_,std::move(environment));
            if (processes>1) return solver.runParallelFlow(caseDir,processes,callbacks,token);
            return solver.execute(exe,exe,args,caseDir,callbacks,token);
        }
    }
    return execute(exe, exe, args, caseDir, callbacks, token);
}

Expected<StepOutcome> CaseRunner::runParallelFlow(const std::filesystem::path& caseDir,
    unsigned processes,const RunCallbacks& callbacks,CancellationToken* token) {
    namespace fs=std::filesystem;
    try {
        for(const auto* tool:{"mpirun","decomposePar","reconstructPar"})
            if(!Detector::resolveExecutable(tool,environment_))
                return Error::make(ErrorCode::BackendUnavailable,std::string("Parallel flow requires ")+tool+"; select one process to run serially");
        auto latestTime=[](const fs::path& root) {
            std::pair<double,fs::path> latest{-1,{}};
            for(const auto& entry:fs::directory_iterator(root)) {
                if(!entry.is_directory())continue;
                const auto name=entry.path().filename().string();
                try {
                    std::size_t used=0;const auto t=std::stod(name,&used);
                    if(used==name.size()&&std::isfinite(t)&&t>=0&&t>latest.first)
                        latest={t,entry.path()};
                } catch(...) {}
            }
            return latest;
        };
        const auto initial=latestTime(caseDir);
        if(initial.first<0) return Error::make(ErrorCode::InvalidData,"No saved flow fields to decompose");
        const auto runs=caseDir/"parallel-runs";
        fs::create_directories(runs);
        fs::path stage;
        for(unsigned attempt=0;attempt<100;++attempt) {
            const auto id=std::chrono::steady_clock::now().time_since_epoch().count();
            stage=runs/("run_"+std::to_string(id)+"_"+std::to_string(attempt));
            if(fs::create_directory(stage))break;
            stage.clear();
        }
        if(stage.empty())return Error::make(ErrorCode::IoError,"Cannot create an isolated parallel run");
        // Original mesh, solver, previous results and processor directories are
        // never overwritten. Retain this run for recovery, even after failure.
        for(const auto* name:{"constant","system"})
            fs::copy(caseDir/name,stage/name,fs::copy_options::recursive);
        fs::copy(initial.second,stage/initial.second.filename(),fs::copy_options::recursive);
        if(fs::is_directory(caseDir/"dynamicCode"))
            fs::copy(caseDir/"dynamicCode",stage/"dynamicCode",fs::copy_options::recursive);
        {
            std::ofstream dict(stage/"system/decomposeParDict",std::ios::trunc);
            dict<<"FoamFile { version 2.0; format ascii; class dictionary; object decomposeParDict; }\n"
                <<"numberOfSubdomains "<<processes<<"; method scotch;\n";
            if(!dict)return Error::make(ErrorCode::IoError,"Cannot write parallel decomposition settings");
        }
        const auto notice="Parallel flow: "+std::to_string(processes)+" processes; recovery folder: "+stage.string()+"\n";
        if(callbacks.onOutput)callbacks.onOutput(OutputStream::StdOut,notice);
        LogFileSink rootLog(caseDir/"log.vortia");rootLog.write(notice);
        auto finishLog=[&]() {
            std::ifstream raw(stage/"log.vortia",std::ios::binary);
            std::string chunk(65536,'\0');
            while(raw.read(chunk.data(),chunk.size())||raw.gcount())
                rootLog.write(std::string_view(chunk.data(),static_cast<std::size_t>(raw.gcount())));
        };
        const auto decomposed=execute("decompose flow","decomposePar",{"-latestTime"},stage,callbacks,token);
        if(!decomposed||!decomposed->process.succeeded()) {finishLog();return decomposed;}
        // A first parallel run may compile boundary libraries while reading
        // fields. Preserve that cache for subsequent resumes of this case.
        if(!fs::exists(caseDir/"dynamicCode")&&fs::is_directory(stage/"dynamicCode")) {
            const auto pending=caseDir/(".dynamicCode_"+stage.filename().string());
            fs::copy(stage/"dynamicCode",pending,fs::copy_options::recursive);
            fs::rename(pending,caseDir/"dynamicCode");
        }
        auto mpiEnvironment=environment_;
        // A desktop can host several independent MPI jobs. Each launcher sees
        // all cores and otherwise busy-spins as if it owned the whole machine.
        // Yield only while waiting inside MPI; preserve an explicit user choice.
        if(std::none_of(mpiEnvironment.begin(),mpiEnvironment.end(),[](const auto& entry) {
            return entry.first=="OMPI_MCA_mpi_yield_when_idle";
        })) mpiEnvironment.emplace_back("OMPI_MCA_mpi_yield_when_idle","1");
        CaseRunner mpi(processes_,std::move(mpiEnvironment));
        auto solved=mpi.execute("parallel flow","mpirun",{"-np",std::to_string(processes),"vortiaCentralFoam","-parallel"},stage,callbacks,token);
        if(!solved||(!solved->process.succeeded()&&!solved->process.cancelled)) {finishLog();return solved;}
        // A cancelled solver may have completed a checkpoint. Reconstruction
        // gets its own uncancelled invocation; failed/partial writes never
        // replace the last usable root result.
        const auto reconstructed=execute("reconstruct saved flow","reconstructPar",{"-latestTime"},stage,callbacks,nullptr);
        finishLog();
        if(!reconstructed||!reconstructed->process.succeeded())return reconstructed;
        const auto latest=latestTime(stage);
        if(latest.first>initial.first) {
            for(const auto* field:{"p","T","U","rho"})
                if(!fs::is_regular_file(latest.second/field)||fs::file_size(latest.second/field)==0)
                    return Error::make(ErrorCode::InvalidData,"Incomplete reconstructed flow checkpoint; recovery files retained",stage.string());
            const auto destination=caseDir/latest.second.filename();
            if(fs::exists(destination))return Error::make(ErrorCode::InvalidData,"A result with this iteration already exists; recovery files retained",stage.string());
            // Publish a complete directory with a single rename on this volume.
            const auto pending=caseDir/(".checkpoint_"+stage.filename().string());
            fs::copy(latest.second,pending,fs::copy_options::recursive);
            fs::rename(pending,destination);
        }
        if(fs::is_directory(stage/"postProcessing")) {
            fs::create_directories(caseDir/"postProcessing");
            fs::copy(stage/"postProcessing",caseDir/"postProcessing"/("parallel_"+stage.filename().string()),fs::copy_options::recursive);
        }
        return solved;
    } catch(const std::exception& e) {
        return Error::make(ErrorCode::Internal,std::string("Parallel flow: ")+e.what());
    }
}

} // namespace vortia::openfoam
