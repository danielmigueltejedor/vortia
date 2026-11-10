#pragma once

#include "vortia/verification/GhiaCavity.hpp"
#include "vortia/verification/Poiseuille.hpp"

#include <sstream>
#include <string>

namespace vortia::verification {

/// OpenFOAM `sets` function object that samples U on a vertical mid-plane line.
[[nodiscard]] inline std::string verticalCentrelineSampleDict(double x, double y0, double y1,
                                                              double z, int nPoints) {
    std::ostringstream out;
    out << "/*--------------------------------*- C++ -*----------------------------------*\\\n"
           "| =========                 |                                                 |\n"
           "| \\\\      /  F ield         | OpenFOAM: The Open Source CFD Toolbox           |\n"
           "|  \\\\    /   O peration     | Version:  v2606                                 |\n"
           "|   \\\\  /    A nd           | Website:  www.openfoam.com                      |\n"
           "|    \\\\/     M anipulation  |                                                 |\n"
           "\\*---------------------------------------------------------------------------*/\n"
           "FoamFile\n"
           "{\n"
           "    version     2.0;\n"
           "    format      ascii;\n"
           "    class       dictionary;\n"
           "    object      sample;\n"
           "}\n"
           "// * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * //\n\n"
           "type            sets;\n"
           "libs            (sampling);\n"
           "writeControl    writeTime;\n"
           "interpolationScheme cellPoint;\n"
           "setFormat       raw;\n"
           "fields          (U);\n"
           "sets\n"
           "(\n"
           "    centreline\n"
           "    {\n"
           "        type            uniform;\n"
           "        axis            y;\n"
           "        start           ("
        << x << " " << y0 << " " << z << ");\n"
        << "        end             (" << x << " " << y1 << " " << z << ");\n"
        << "        nPoints         " << nPoints << ";\n"
           "    }\n"
           ");\n\n"
           "// ************************************************************************* //\n";
    return out.str();
}

[[nodiscard]] inline std::string horizontalCentrelineSampleDict(double y, double x0, double x1,
                                                                double z, int nPoints) {
    std::ostringstream out;
    out << "/*--------------------------------*- C++ -*----------------------------------*\\\n"
           "| =========                 |                                                 |\n"
           "| \\\\      /  F ield         | OpenFOAM: The Open Source CFD Toolbox           |\n"
           "|  \\\\    /   O peration     | Version:  v2606                                 |\n"
           "|   \\\\  /    A nd           | Website:  www.openfoam.com                      |\n"
           "|    \\\\/     M anipulation  |                                                 |\n"
           "\\*---------------------------------------------------------------------------*/\n"
           "FoamFile\n"
           "{\n"
           "    version     2.0;\n"
           "    format      ascii;\n"
           "    class       dictionary;\n"
           "    object      sample;\n"
           "}\n"
           "// * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * //\n\n"
           "type            sets;\n"
           "libs            (sampling);\n"
           "writeControl    writeTime;\n"
           "interpolationScheme cellPoint;\n"
           "setFormat       raw;\n"
           "fields          (U);\n"
           "sets\n"
           "(\n"
           "    centreline\n"
           "    {\n"
           "        type            uniform;\n"
           "        axis            x;\n"
           "        start           ("
        << x0 << " " << y << " " << z << ");\n"
        << "        end             (" << x1 << " " << y << " " << z << ");\n"
        << "        nPoints         " << nPoints << ";\n"
           "    }\n"
           ");\n\n"
           "// ************************************************************************* //\n";
    return out.str();
}

/// The uniform line set is called `uniform` in ESI OpenFOAM (the default
/// written above; verified with v2412) and `lineUniform` in Foundation
/// releases. Returns the dictionary adapted for Foundation.
[[nodiscard]] inline std::string sampleDictForFoundation(std::string dict) {
    const std::string esi = "type            uniform;";
    for (auto at = dict.find(esi); at != std::string::npos; at = dict.find(esi, at))
        dict.replace(at, esi.size(), "type            lineUniform;");
    return dict;
}

[[nodiscard]] inline std::string poiseuilleSampleDict(const PoiseuilleChannel& c) {
    const double z = 0.5 * c.depth;
    return verticalCentrelineSampleDict(0.5 * c.length, 0.0, c.height, z, 80);
}

[[nodiscard]] inline std::string ghiaUSampleDict() {
    // Domain H=0.1, depth 0.01 → sample at mid-z.
    return verticalCentrelineSampleDict(0.05, 0.0, 0.1, 0.005, 120);
}

[[nodiscard]] inline std::string ghiaVSampleDict() {
    return horizontalCentrelineSampleDict(0.05, 0.0, 0.1, 0.005, 120);
}

} // namespace vortia::verification
