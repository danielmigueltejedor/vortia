#include "vortia/mesh/PlanarNozzleBlockMesh.hpp"

#include <cmath>
#include <iomanip>
#include <limits>
#include <sstream>
#include <vector>

namespace vortia {
namespace {

struct Station {
    double x;
    double radius;
};

std::string number(double value) {
    std::ostringstream out;
    out.imbue(std::locale::classic());
    out << std::setprecision(12) << value;
    return out.str();
}

void face(std::ostringstream& out, int a, int b, int c, int d) {
    out << "        (" << a << " " << b << " " << c << " " << d << ")\n";
}

} // namespace

Expected<std::string> PlanarNozzleBlockMesh::generate(
    const NozzleDimensions& dims, const PlanarNozzleMeshOptions& mesh) {
    if (!makeNozzleProfile(dims))
        return Error::make(ErrorCode::InvalidArgument, "Invalid nozzle dimensions");
    if (mesh.convergingSegments < 2 || mesh.convergingSegments > 200 ||
        mesh.divergingSegments < 2 || mesh.divergingSegments > 200 ||
        mesh.axialCellsPerSegment < 1 || mesh.axialCellsPerSegment > 200 ||
        mesh.radialCells < 2 || mesh.radialCells > 1000 ||
        !std::isfinite(mesh.radialGrading) || mesh.radialGrading <= 0.0 ||
        mesh.radialGrading > 1000.0 ||
        !std::isfinite(mesh.thicknessMm) || mesh.thicknessMm <= 0.0 ||
        mesh.thicknessMm > 1000.0)
        return Error::make(ErrorCode::InvalidArgument, "Invalid planar nozzle meshing options");

    const int totalSegments = mesh.convergingSegments + mesh.divergingSegments;
    const long long cellCount = static_cast<long long>(totalSegments) *
                                mesh.axialCellsPerSegment * mesh.radialCells;
    if (cellCount > 2000000)
        return Error::make(ErrorCode::InvalidArgument,
                           "Planar nozzle mesh exceeds 2,000,000 cells");

    const auto easing = [](double t) { return t * t * (3.0 - 2.0 * t); };
    std::vector<Station> stations;
    stations.reserve(static_cast<std::size_t>(totalSegments + 1));
    const double ri = dims.inletDiameterMm / 2.0;
    const double rt = dims.throatDiameterMm / 2.0;
    const double ro = dims.outletDiameterMm / 2.0;
    for (int i = 0; i <= mesh.convergingSegments; ++i) {
        const double t = static_cast<double>(i) / mesh.convergingSegments;
        stations.push_back({dims.convergingLengthMm * t, ri + (rt - ri) * easing(t)});
    }
    for (int i = 1; i <= mesh.divergingSegments; ++i) {
        const double t = static_cast<double>(i) / mesh.divergingSegments;
        stations.push_back({dims.convergingLengthMm + dims.divergingLengthMm * t,
                            rt + (ro - rt) * easing(t)});
    }

    std::ostringstream out;
    out.imbue(std::locale::classic());
    out << "/*--------------------------------*- C++ -*----------------------------------*\\\\\n"
        << "| Vortia planar nozzle (NOT a rotational wedge or compressible setup)         |\n"
        << "\\*---------------------------------------------------------------------------*/\n"
        << "FoamFile\n{\n    version 2.0;\n    format ascii;\n"
        << "    class dictionary;\n    object blockMeshDict;\n}\n\n"
        << "convertToMeters 0.001;\n\nvertices\n(\n";
    // Four vertices per station:
    // 0 axis/front, 1 wall/front, 2 axis/back, 3 wall/back.
    // All adjacent blocks share their station vertices (conforming mesh).
    for (const auto& station : stations) {
        out << "    (" << number(station.x) << " 0 0)\n";
        out << "    (" << number(station.x) << " " << number(station.radius) << " 0)\n";
        out << "    (" << number(station.x) << " 0 " << number(mesh.thicknessMm) << ")\n";
        out << "    (" << number(station.x) << " " << number(station.radius) << " "
            << number(mesh.thicknessMm) << ")\n";
    }
    out << ");\n\nblocks\n(\n";
    for (int i = 0; i < totalSegments; ++i) {
        const int a = i * 4;
        const int b = (i + 1) * 4;
        out << "    hex (" << a << " " << b << " " << b+1 << " " << a+1
            << " " << a+2 << " " << b+2 << " " << b+3 << " " << a+3
            << ") (" << mesh.axialCellsPerSegment << " " << mesh.radialCells
            << " 1) simpleGrading (1 " << number(mesh.radialGrading) << " 1)\n";
    }
    out << ");\n\nboundary\n(\n"
        << "    inlet\n    {\n        type patch;\n        faces\n        (\n";
    face(out, 0, 2, 3, 1);
    out << "        );\n    }\n"
        << "    outlet\n    {\n        type patch;\n        faces\n        (\n";
    const int last = totalSegments * 4;
    face(out, last, last+1, last+3, last+2);
    out << "        );\n    }\n"
        << "    axis\n    {\n        type symmetryPlane;\n        faces\n        (\n";
    for (int i = 0; i < totalSegments; ++i) {
        const int a = i * 4, b = (i + 1) * 4;
        face(out, a, b, b+2, a+2);
    }
    out << "        );\n    }\n"
        << "    nozzleWall\n    {\n        type wall;\n        faces\n        (\n";
    for (int i = 0; i < totalSegments; ++i) {
        const int a = i * 4, b = (i + 1) * 4;
        face(out, a+1, a+3, b+3, b+1);
    }
    out << "        );\n    }\n"
        << "    frontAndBack\n    {\n        type empty;\n        faces\n        (\n";
    for (int i = 0; i < totalSegments; ++i) {
        const int a = i * 4, b = (i + 1) * 4;
        face(out, a, a+1, b+1, b);
        face(out, a+2, b+2, b+3, a+3);
    }
    out << "        );\n    }\n);\n\n"
        << "// Mesh is planar, NOT axisymmetric. Do not model high-speed nozzle physics\n"
        << "// using Vortia's current incompressible icoFoam/simpleFoam presets.\n"
        << "// ************************************************************************* //\n";
    return out.str();
}

} // namespace vortia
