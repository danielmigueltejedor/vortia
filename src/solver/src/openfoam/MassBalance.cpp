#include "vortia/solver/openfoam/MassBalance.hpp"

#include <filesystem>
#include <fstream>
#include <sstream>
#include <string>

namespace vortia::openfoam {

namespace {

[[nodiscard]] bool fileExists(const std::filesystem::path& p) {
    std::error_code ec;
    return std::filesystem::exists(p, ec) && !ec;
}

} // namespace

bool isNozzlePracticeCase(const std::filesystem::path& caseDir) {
    if (caseDir.empty()) return false;
    // A nozzle-practice case is identified by the presence of a nozzle-related
    // boundary condition or a marker file in the case directory.
    return fileExists(caseDir / "0" / "U") &&
           fileExists(caseDir / "0" / "p") &&
           fileExists(caseDir / "system" / "controlDict");
}

MassBalanceResult nozzleMassBalance(const std::filesystem::path& caseDir) {
    MassBalanceResult result;
    if (caseDir.empty()) return result;

    // Look for mass-balance data in common OpenFOAM post-processing locations.
    const std::filesystem::path postDir = caseDir / "postProcessing";
    if (!fileExists(postDir)) return result;

    // Try to find a massBalance or massFlow directory.
    std::filesystem::path balanceFile;
    for (const auto& entry : std::filesystem::directory_iterator(postDir)) {
        if (!entry.is_directory()) continue;
        const std::string name = entry.path().filename().string();
        if (name.find("mass") != std::string::npos) {
            // Look for a data file inside.
            for (const auto& sub : std::filesystem::directory_iterator(entry.path())) {
                if (sub.is_regular_file() && sub.path().extension() == ".dat") {
                    balanceFile = sub.path();
                    break;
                }
            }
            if (!balanceFile.empty()) break;
        }
    }

    if (balanceFile.empty()) return result;

    // Parse the mass balance file (OpenFOAM format: time inlet outlet imbalance).
    std::ifstream in(balanceFile);
    if (!in) return result;

    double sumAbs = 0.0;
    double maxAbs = 0.0;
    int count = 0;
    int first = 0;
    int last = 0;
    std::string line;
    while (std::getline(in, line)) {
        if (line.empty() || line[0] == '#') continue;
        std::istringstream iss(line);
        double time = 0.0, inlet = 0.0, outlet = 0.0;
        if (!(iss >> time >> inlet >> outlet)) continue;
        if (inlet == 0.0) continue;
        const double imbalance = std::abs((inlet - outlet) / inlet) * 100.0;
        sumAbs += imbalance;
        if (imbalance > maxAbs) maxAbs = imbalance;
        if (count == 0) first = static_cast<int>(time);
        last = static_cast<int>(time);
        ++count;
    }

    if (count > 0) {
        result.available = true;
        result.firstIteration = first;
        result.lastIteration = last;
        result.meanAbsolutePercent = sumAbs / count;
        result.maximumAbsolutePercent = maxAbs;
        result.samples = count;
    }
    return result;
}

} // namespace vortia::openfoam
