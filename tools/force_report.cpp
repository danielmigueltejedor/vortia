// Reads an OpenFOAM forces file with Vortia's parser and, for the Poiseuille
// benchmark, compares the wall force with the exact momentum balance.
// Usage: force_report <force.dat>
//        force_report poiseuille <force.dat> [relative-tolerance]
// Exit codes: 0 ok / pass, 1 fail or unreadable, 2 usage.
#include "vortia/solver/openfoam/Forces.hpp"
#include "vortia/verification/Poiseuille.hpp"

#include <cmath>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>

namespace {

bool readFile(const std::string& path, std::string& text) {
    std::ifstream in(path, std::ios::binary);
    if (!in)
        return false;
    std::ostringstream buffer;
    buffer << in.rdbuf();
    text = buffer.str();
    return true;
}

void print(const char* label, const vortia::Vec3& v) {
    std::cout << label << " = (" << v.x << ' ' << v.y << ' ' << v.z << ")\n";
}

} // namespace

int main(int argc, char** argv) {
    const bool poiseuille = argc >= 3 && std::string(argv[1]) == "poiseuille";
    if (!(argc == 2 || (poiseuille && argc <= 4))) {
        std::cerr << "Usage: force_report <force.dat>\n"
                     "       force_report poiseuille <force.dat> [relative-tolerance]\n";
        return 2;
    }
    const std::string path = poiseuille ? argv[2] : argv[1];
    std::string text;
    if (!readFile(path, text)) {
        std::cerr << "Cannot read " << path << "\n";
        return 1;
    }
    const auto rows = vortia::openfoam::parseForces(text);
    if (!rows) {
        std::cerr << rows.error().describe() << "\n";
        return 1;
    }
    const auto& last = rows->back();
    std::cout.precision(10);
    std::cout << "time = " << last.time << " (" << rows->size() << " rows)\n";
    print("total", last.total);
    print("pressure", last.pressure);
    print("viscous", last.viscous);
    if (!poiseuille)
        return 0;

    // Fully developed channel, force per unit density on both walls:
    // the wall shear balances the pressure drop, F_x = (p_in - p_out) H W.
    const vortia::verification::PoiseuilleChannel channel;
    const double expected =
        (channel.pressureInlet - channel.pressureOutlet) * channel.height * channel.depth;
    const double tolerance = argc == 4 ? std::strtod(argv[3], nullptr) : 0.02;
    if (!(tolerance > 0.0) || !(tolerance < 1.0)) {
        std::cerr << "Invalid tolerance\n";
        return 2;
    }
    const double relative = std::abs(last.viscous.x - expected) / expected;
    // Walls are normal to y: pressure contributes nothing along x.
    const double pressureLeak = std::abs(last.pressure.x) / expected;
    std::cout << "analytic viscous_x = " << expected << "\n"
              << "relative error = " << relative << " (limit " << tolerance << ")\n"
              << "pressure_x / analytic = " << pressureLeak << " (limit 1e-6)\n";
    if (relative > tolerance || pressureLeak > 1e-6) {
        std::cerr << "FAIL: wall force disagrees with the momentum balance\n";
        return 1;
    }
    std::cout << "PASS\n";
    return 0;
}
