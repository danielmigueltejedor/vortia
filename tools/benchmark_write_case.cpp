// Writes a Vortia verification case (Poiseuille or Ghia cavity) for OpenFOAM.
// Usage: benchmark_write_case <poiseuille|ghia|blasius> <caseDir> [esi|foundation]
#include "vortia/solver/openfoam/CaseWriter.hpp"
#include "vortia/solver/openfoam/Installation.hpp"
#include "vortia/verification/Blasius.hpp"
#include "vortia/verification/GhiaCavity.hpp"
#include "vortia/verification/Poiseuille.hpp"
#include "vortia/verification/SampleFunctions.hpp"

#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>
#include <string_view>

namespace {

bool writeText(const std::filesystem::path& path, const std::string& contents) {
    std::filesystem::create_directories(path.parent_path());
    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
        return false;
    out << contents;
    return static_cast<bool>(out);
}

vortia::openfoam::Capabilities capsFor(std::string_view dist) {
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

} // namespace

int main(int argc, char** argv) {
    if (argc < 3 || argc > 4) {
        std::cerr << "Usage: benchmark_write_case <poiseuille|ghia|blasius> <caseDir> [esi|foundation]\n";
        return 2;
    }
    const std::string kind = argv[1];
    const std::filesystem::path caseDir = argv[2];
    // Default ESI; scripts pass foundation explicitly when foamRun is selected.
    const std::string dist = (argc == 4) ? argv[3] : "esi";
    if (dist != "esi" && dist != "foundation") {
        std::cerr << "Unknown distribution: " << dist << " (expected esi|foundation)\n";
        return 2;
    }

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
    } else if (kind == "blasius") {
        setup = vortia::verification::makeBlasiusCase(vortia::verification::BlasiusPlate{}, {80, 60, 1});
        // Wall-normal sample at x = L/2, concentrated inside η ≲ 5 (≈ δ99).
        const vortia::verification::BlasiusPlate plate;
        const double x = 0.5 * plate.length;
        const double yEnd = vortia::verification::delta99(plate, x);
        sampleU = vortia::verification::verticalCentrelineSampleDict(
            x, 0.0, yEnd, 0.5 * plate.depth, 80);
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
