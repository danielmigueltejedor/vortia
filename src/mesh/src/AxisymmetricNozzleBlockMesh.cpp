#include "vortia/mesh/AxisymmetricNozzleBlockMesh.hpp"

#include <cmath>
#include <iomanip>
#include <locale>
#include <sstream>
#include <vector>

namespace vortia {

namespace {

struct Station { double x; double r; };

std::string fmt(double x) {
    std::ostringstream out;
    out.imbue(std::locale::classic());
    out << std::setprecision(12) << x;
    return out.str();
}

void patchFace(std::ostringstream& out, int a, int b, int c, int d) {
    out << "        (" << a << " " << b << " " << c << " " << d << ")\n";
}

} // namespace

Expected<std::string> AxisymmetricNozzleBlockMesh::generate(
    const NozzleDimensions& dims, const AxisymmetricNozzleMeshOptions& o) {
    if (!makeNozzleProfile(dims))
        return Error::make(ErrorCode::InvalidArgument, "Invalid nozzle geometry");
    if (o.convergingSegments < 2 || o.convergingSegments > 200 ||
        o.divergingSegments < 2 || o.divergingSegments > 200 ||
        o.axialCellsPerSegment < 1 || o.axialCellsPerSegment > 200 ||
        o.radialCells < 2 || o.radialCells > 1000 ||
        !std::isfinite(o.radialGrading) || o.radialGrading <= 0.0 ||
        o.radialGrading > 1000.0 ||
        !std::isfinite(o.wedgeAngleDegrees) || o.wedgeAngleDegrees <= 0.01 ||
        o.wedgeAngleDegrees > 5.0)
        return Error::make(ErrorCode::InvalidArgument, "Invalid wedge mesh options");

    const int n = o.convergingSegments + o.divergingSegments;
    const auto cells = static_cast<long long>(n) * o.axialCellsPerSegment * o.radialCells;
    if (cells > 2000000)
        return Error::make(ErrorCode::InvalidArgument, "Wedge mesh exceeds 2,000,000 cells");

    const auto smooth = [](double t) { return t*t*(3.0-2.0*t); };
    const double ri = dims.inletDiameterMm/2.0;
    const double rt = dims.throatDiameterMm/2.0;
    const double ro = dims.outletDiameterMm/2.0;
    std::vector<Station> stations;
    stations.reserve(static_cast<std::size_t>(n+1));
    for(int i=0;i<=o.convergingSegments;++i) {
        const double t=static_cast<double>(i)/o.convergingSegments;
        stations.push_back({dims.convergingLengthMm*t,ri+(rt-ri)*smooth(t)});
    }
    for(int i=1;i<=o.divergingSegments;++i) {
        const double t=static_cast<double>(i)/o.divergingSegments;
        stations.push_back({dims.convergingLengthMm+dims.divergingLengthMm*t,
                            rt+(ro-rt)*smooth(t)});
    }

    constexpr double pi = 3.141592653589793238462643383279502884;
    const double halfAngle = o.wedgeAngleDegrees*pi/360.0;
    const double cosine = std::cos(halfAngle);
    const double sine = std::sin(halfAngle);

    std::ostringstream out;
    out.imbue(std::locale::classic());
    out << "/*--------------------------------*- C++ -*----------------------------------*\\\\\n"
        << "| Vortia rotationally axisymmetric wedge: validate with blockMesh/checkMesh.  |\n"
        << "\\*---------------------------------------------------------------------------*/\n"
        << "FoamFile\n{\n"
        << "    version 2.0;\n    format ascii;\n"
        << "    class dictionary;\n    object blockMeshDict;\n}\n\n"
        << "convertToMeters 0.001;\n\nvertices\n(\n";

    // Three *unique* vertices per axial station:
    // axis (x,0,0), front wall (x,r*cos(a),-r*sin(a)),
    // back wall (x,r*cos(a),+r*sin(a)).
    // The axial vertices collapse in the hex ordering to a zero-area
    // empty axis patch, as required by OpenFOAM's wedge geometry.
    for (const auto& st : stations) {
        out << "    (" << fmt(st.x) << " 0 0)\n";
        out << "    (" << fmt(st.x) << " " << fmt(st.r*cosine)
            << " " << fmt(-st.r*sine) << ")\n";
        out << "    (" << fmt(st.x) << " " << fmt(st.r*cosine)
            << " " << fmt(st.r*sine) << ")\n";
    }
    out << ");\n\nblocks\n(\n";
    for(int i=0;i<n;++i) {
        const int a=3*i, b=3*(i+1);
        out << "    hex (" << a << " " << b << " " << b+1 << " " << a+1
            << " " << a << " " << b << " " << b+2 << " " << a+2
            << ") (" << o.axialCellsPerSegment << " " << o.radialCells
            << " 1) simpleGrading (1 " << fmt(o.radialGrading) << " 1)\n";
    }
    out << ");\n\nboundary\n(\n"
        << "    inlet\n    {\n        type patch;\n        faces\n        (\n";
    patchFace(out,0,0,2,1);
    out << "        );\n    }\n"
        << "    outlet\n    {\n        type patch;\n        faces\n        (\n";
    const int last=3*n;
    patchFace(out,last,last+1,last+2,last);
    out << "        );\n    }\n"
        << "    nozzleWall\n    {\n        type wall;\n        faces\n        (\n";
    for(int i=0;i<n;++i) {
        const int a=3*i,b=3*(i+1);
        patchFace(out,a+1,a+2,b+2,b+1);
    }
    out << "        );\n    }\n"
        << "    wedgeFront\n    {\n        type wedge;\n        faces\n        (\n";
    for(int i=0;i<n;++i) {
        const int a=3*i,b=3*(i+1);
        patchFace(out,a,a+1,b+1,b);
    }
    out << "        );\n    }\n"
        << "    wedgeBack\n    {\n        type wedge;\n        faces\n        (\n";
    for(int i=0;i<n;++i) {
        const int a=3*i,b=3*(i+1);
        patchFace(out,a,b,b+2,a+2);
    }
    out << "        );\n    }\n"
        << "    axis\n    {\n        type empty;\n        faces\n        (\n";
    // Collapsed axis faces have zero area. blockMesh omits them from polyMesh.
    for(int i=0;i<n;++i) {
        const int a=3*i,b=3*(i+1);
        patchFace(out,a,b,b,a);
    }
    out << "        );\n    }\n);\n\n"
        << "// Mesh only: no compressible solver, no energy equation, no BC fields.\n"
        << "// ************************************************************************* //\n";
    return out.str();
}

} // namespace vortia
