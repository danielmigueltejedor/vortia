#include "vortia/mesh/AxisymmetricNozzleBlockMesh.hpp"

#include <fstream>
#include <iostream>

int main(int argc, char** argv) {
    if (argc != 2) {
        std::cerr << "usage: nozzle_mesh_export_smoke <system/blockMeshDict>\n";
        return 2;
    }
    const vortia::NozzleDimensions nozzle{100, 52, 154, 160, 418};
    const vortia::AxisymmetricNozzleMeshOptions mesh{8, 12, 2, 24, 1.8, 5};
    const auto dictionary = vortia::AxisymmetricNozzleBlockMesh::generate(nozzle,mesh);
    if (!dictionary) {
        std::cerr << dictionary.error().describe() << "\n";
        return 3;
    }
    std::ofstream out(argv[1], std::ios::binary);
    if (!out) return 4;
    out << dictionary.value();
    out.close();
    return out ? 0 : 5;
}
