// Writes a Vortia verification case (Poiseuille or Ghia cavity) for OpenFOAM.
// Usage: benchmark_write_case <poiseuille|ghia> <caseDir>
#include "vortia/solver/openfoam/CaseWriter.hpp"
#include "vortia/solver/openfoam/Installation.hpp"
#include "vortia/verification/GhiaCavity.hpp"
#include "vortia/verification/Poiseuille.hpp"
#include "vortia/verification/SampleFunctions.hpp"

#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>

namespace {

bool writeText(const std::filesystem::path& path, const std::string& contents) {
    std::filesystem::create_directories(path.parent_path());
    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
        return false;
    out << contents;
    return static_cast<bool>(out);
}

vortia::openfoam::Capabilities esiCaps() {
    vortia::openfoam::Installation i;
    i.distribution = vortia::openfoam::Distribution::Esi;
    i.version = vortia::Version::parse("v2606").value();
    return vortia::openfoam::capabilitiesFor(i);
}

} // namespace

int main(int argc, char** argv) {
    if (argc != 3) {
        std::cerr << "Usage: benchmark_write_case <poiseuille|ghia> <caseDir>\n";
        return 2;
    }
    const std::string kind = argv[1];
    const std::filesystem::path caseDir = argv[2];

    vortia::CaseSetup setup;
    std::string sampleU;
    std::string sampleV;
    if (kind == "poiseuille") {
        const vortia::verification::PoiseuilleChannel channel;
        setup = vortia::verification::makePoiseuilleCase(channel, {60, 40, 1});
        sampleU = vortia::verification::poiseuilleSampleDict(channel);
    } else if (kind == "ghia") {
        setup = vortia::verification::makeGhiaCavityCase({64, 64, 1});
        sampleU = vortia::verification::ghiaUSampleDict();
        sampleV = vortia::verification::ghiaVSampleDict();
    } else {
        std::cerr << "Unknown case kind: " << kind << "\n";
        return 2;
    }

    const auto caps = esiCaps();
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

    std::cout << "Wrote " << kind << " case to " << caseDir << "\n";
    return 0;
}
