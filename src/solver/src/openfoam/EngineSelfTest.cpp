#include "vortia/solver/openfoam/EngineSelfTest.hpp"

#include "vortia/solver/openfoam/CaseWriter.hpp"
#include "vortia/solver/openfoam/Detector.hpp"
#include "vortia/verification/GhiaCavity.hpp"

#include <chrono>
#include <fstream>

namespace vortia::openfoam {

namespace {

/// Result directories of a case: numeric names other than the initial "0".
std::vector<std::filesystem::path> resultDirectories(const std::filesystem::path& base) {
    std::vector<std::filesystem::path> found;
    std::error_code ec;
    for (std::filesystem::directory_iterator it(base, ec), end; !ec && it != end; it.increment(ec)) {
        const std::string name = it->path().filename().string();
        if (!it->is_directory(ec) || name == "0" || name.empty())
            continue;
        if (name.find_first_not_of("0123456789.") != std::string::npos)
            continue;
        if (std::filesystem::is_regular_file(it->path() / "U", ec))
            found.push_back(it->path());
    }
    return found;
}

std::string tail(const std::string& text, std::size_t maxBytes = 2000) {
    return text.size() <= maxBytes ? text : text.substr(text.size() - maxBytes);
}

} // namespace

Expected<EngineSelfTestReport>
runEngineSelfTest(const Installation& installation, IProcessRunner& runner,
                  const std::filesystem::path& workDir, int processes) {
    if (processes < 2 || processes > 8)
        return Error::make(ErrorCode::InvalidArgument, "self-test needs 2 to 8 processes");
    std::error_code ec;
    if (!std::filesystem::is_directory(workDir, ec) || !std::filesystem::is_empty(workDir, ec))
        return Error::make(ErrorCode::InvalidArgument,
                           "self-test work directory must exist and be empty", workDir.string());

    const auto environment = Detector::captureEnvironment(installation, runner);
    if (!environment)
        return environment.error();
    const Capabilities capabilities = capabilitiesFor(installation);

    // Small enough to converge in seconds on any machine.
    const CaseSetup setup = verification::makeGhiaCavityCase({16, 16, 1});
    const std::filesystem::path caseDir = workDir / "cavity";
    if (const auto written = CaseWriter::write(caseDir, setup, capabilities, {}); !written)
        return written.error();

    EngineSelfTestReport report;
    const auto step = [&](const std::string& label, const std::string& tool,
                          std::vector<std::string> arguments) -> Expected<void> {
        const auto executable = Detector::resolveExecutable(tool, *environment);
        if (!executable)
            return executable.error();
        ProcessRequest request;
        request.executable = *executable;
        request.arguments = std::move(arguments);
        request.workingDirectory = caseDir;
        request.environment = *environment;
        request.timeout = std::chrono::seconds{180};
        const auto result = runner.run(request);
        if (!result)
            return result.error();
        if (!result->succeeded())
            return Error::make(result->timedOut ? ErrorCode::Timeout : ErrorCode::ProcessFailed,
                               label + " failed (" + tool + ")",
                               tail(result->stdOut) + "\n" + tail(result->stdErr));
        report.steps.push_back(label + ": " + executable->string());
        return {};
    };

    const std::string solver = capabilities.laminarSteadySolver;
    const std::vector<std::string> solverArgs = capabilities.laminarSteadyArgs;
    if (auto done = step("mesh", "blockMesh", {}); !done)
        return done.error();
    if (auto done = step("mesh check", "checkMesh", {}); !done)
        return done.error();
    if (auto done = step("serial solve", solver, solverArgs); !done)
        return done.error();
    const auto serialResults = resultDirectories(caseDir);
    if (serialResults.empty())
        return Error::make(ErrorCode::ProcessFailed, "serial solve wrote no velocity field",
                           caseDir.string());
    // The parallel run must produce its own result, not find the serial one.
    for (const auto& directory : serialResults)
        std::filesystem::remove_all(directory, ec);

    {
        std::ofstream dict(caseDir / "system" / "decomposeParDict", std::ios::trunc);
        dict << "FoamFile { version 2.0; format ascii; class dictionary; object decomposeParDict; }\n"
             << "numberOfSubdomains " << processes << ";\nmethod scotch;\n";
        if (!dict)
            return Error::make(ErrorCode::IoError, "cannot write decomposeParDict", caseDir.string());
    }
    if (auto done = step("decompose", "decomposePar", {}); !done)
        return done.error();
    std::vector<std::string> parallel = {"-np", std::to_string(processes), solver};
    parallel.insert(parallel.end(), solverArgs.begin(), solverArgs.end());
    parallel.emplace_back("-parallel");
    if (auto done = step("parallel solve (" + std::to_string(processes) + " MPI processes)",
                         "mpirun", parallel);
        !done)
        return done.error();
    if (auto done = step("reconstruct", "reconstructPar", {"-latestTime"}); !done)
        return done.error();
    if (resultDirectories(caseDir).empty())
        return Error::make(ErrorCode::ProcessFailed, "parallel solve wrote no velocity field",
                           caseDir.string());
    if (auto done = step("export for the viewer", "foamToVTK", {"-latestTime"}); !done)
        return done.error();
    return report;
}

} // namespace vortia::openfoam
