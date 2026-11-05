// Writes a Vortia verification case (Poiseuille or Ghia cavity) for OpenFOAM.
// Usage: benchmark_write_case <poiseuille|ghia> <caseDir> [esi|foundation]
#include "vortia/solver/openfoam/CaseWriter.hpp"
#include "vortia/solver/openfoam/Installation.hpp"
#include "vortia/verification/GhiaCavity.hpp"
#include "vortia/verification/Poiseuille.hpp"
#include "vortia/verification/SampleFunctions.hpp"

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>
#include <unistd.h>

namespace {

bool writeText(const std::filesystem::path& path, const std::string& contents) {
    std::filesystem::create_directories(path.parent_path());
    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
        return false;
    out << contents;
    return static_cast<bool>(out);
}

bool commandOnPath(const char* name) {
    const char* pathEnv = std::getenv("PATH");
    if (!pathEnv)
        return false;
    std::string path = pathEnv;
    std::size_t start = 0;
    while (start <= path.size()) {
        const auto end = path.find(':', start);
        const auto dir = path.substr(start, end == std::string::npos ? std::string::npos : end - start);
        if (!dir.empty()) {
            const auto candidate = (std::filesystem::path(dir) / name).string();
            if (::access(candidate.c_str(), X_OK) == 0)
                return true;
        }
        if (end == std::string::npos)
            break;
        start = end + 1;
    }
    return false;
}

vortia::openfoam::Capabilities capsFor(const std::string& dist) {
    vortia::openfoam::Installation i;
    if (dist == "foundation") {
        i.distribution = vortia::openfoam::Distribution::Foundation;
        i.version = vortia::Version::parse("11").value();
    } else {
        i.distribution = vortia::openfoam::Distribution::Esi;
        i.version = vortia::Version::parse("v2606").value();
    }
    return vortia::openfoam::capabilitiesFor(i);
}

std::string detectDistribution(const std::string& explicitDist) {
    if (!explicitDist.empty())
        return explicitDist;
    if (const char* env = std::getenv("VORTIA_BENCHMARK_DISTRIBUTION")) {
        const std::string v = env;
        if (v == "esi" || v == "foundation")
            return v;
    }
    // Prefer classic solvers when present; otherwise Foundation module layout.
    if (commandOnPath("simpleFoam"))
        return "esi";
    if (commandOnPath("foamRun"))
        return "foundation";
    return "esi";
}

} // namespace

int main(int argc, char** argv) {
    if (argc < 3 || argc > 4) {
        std::cerr << "Usage: benchmark_write_case <poiseuille|ghia> <caseDir> [esi|foundation]\n";
        return 2;
    }
    const std::string kind = argv[1];
    const std::filesystem::path caseDir = argv[2];
    const std::string dist = detectDistribution(argc == 4 ? argv[3] : "");

    vortia::CaseSetup setup;
    std::string sampleU;
    std::string sampleV;
    if (kind == "poiseuille") {
        const vortia::verification::PoiseuilleChannel channel;
        setup = vortia::verification::makePoiseuilleCase(channel, {60, 40, 1});
        sampleU = vortia::verification::poiseuilleSampleDict(channel);
    } else if (kind == "ghia") {
        setup = vortia::verification::makeGhiaCavityCase({48, 48, 1});
        sampleU = vortia::verification::ghiaUSampleDict();
        sampleV = vortia::verification::ghiaVSampleDict();
    } else {
        std::cerr << "Unknown case kind: " << kind << "\n";
        return 2;
    }

    const auto caps = capsFor(dist);
    vortia::openfoam::WriteOptions opts;
    opts.overwrite = true;
    const auto written = vortia::openfoam::CaseWriter::write(caseDir, setup, caps, opts);
    if (!written) {
        std::cerr << written.error().describe() << "\n";
        return 1;
    }

    if (!writeText(caseDir / "system" / "sampleU", sampleU)) {
        std::cerr << "Failed to write system/sampleU\n";
        return 1;
    }
    if (!sampleV.empty() && !writeText(caseDir / "system" / "sampleV", sampleV)) {
        std::cerr << "Failed to write system/sampleV\n";
        return 1;
    }

    std::cout << "Wrote " << kind << " case (" << dist << ") to " << caseDir << "\n";
    std::cout << "application=" << (setup.numerics.ddt == vortia::TimeScheme::SteadyState
                                        ? caps.laminarSteadySolver
                                        : caps.laminarTransientSolver)
              << "\n";
    return 0;
}
