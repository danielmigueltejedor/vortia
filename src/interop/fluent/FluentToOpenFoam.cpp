#include "vortia/interop/fluent/FluentToOpenFoam.hpp"

#include "vortia/core/Logger.hpp"

#include <algorithm>
#include <fstream>
#include <map>
#include <set>
#include <sstream>
#include <vector>

namespace vortia::fluent {

namespace {

// ---------------------------------------------------------------------------
// OpenFOAM polyMesh writer (raw format) for tet/hex/pyramid/wedge cells.
// ---------------------------------------------------------------------------

struct FaceKey {
    std::vector<std::uint32_t> nodes; // sorted
    bool operator<(const FaceKey& o) const { return nodes < o.nodes; }
};

// Generates the ordered list of node indices (into the points array) that
// form each face of a cell, given the cell's node indices (already mapped to
// points indices). Returns a list of faces, each a vector of point indices.
std::vector<std::vector<std::uint32_t>> cellFaces(FluentCellType type,
                                                   const std::vector<std::uint32_t>& n) {
    std::vector<std::vector<std::uint32_t>> out;
    switch (type) {
    case FluentCellType::Tetra: // 4 nodes: 0-1-2, 0-1-3, 0-2-3, 1-2-3
        if (n.size() >= 4) {
            out.push_back({n[0], n[1], n[2]});
            out.push_back({n[0], n[1], n[3]});
            out.push_back({n[0], n[2], n[3]});
            out.push_back({n[1], n[2], n[3]});
        }
        break;
    case FluentCellType::Hexa: // 8 nodes, 6 quad faces
        if (n.size() >= 8) {
            out.push_back({n[0], n[3], n[2], n[1]}); // bottom
            out.push_back({n[4], n[5], n[6], n[7]}); // top
            out.push_back({n[0], n[1], n[5], n[4]}); // front
            out.push_back({n[2], n[3], n[7], n[6]}); // back
            out.push_back({n[1], n[2], n[6], n[5]}); // right
            out.push_back({n[0], n[4], n[7], n[3]}); // left
        }
        break;
    case FluentCellType::Pyramid: // 5 nodes: base quad + 4 tri
        if (n.size() >= 5) {
            out.push_back({n[0], n[3], n[2], n[1]}); // base
            out.push_back({n[0], n[1], n[4]});
            out.push_back({n[1], n[2], n[4]});
            out.push_back({n[2], n[3], n[4]});
            out.push_back({n[3], n[0], n[4]});
        }
        break;
    case FluentCellType::Wedge: // 6 nodes: 2 tri + 3 quad
        if (n.size() >= 6) {
            out.push_back({n[0], n[2], n[1]}); // tri bottom
            out.push_back({n[3], n[4], n[5]}); // tri top
            out.push_back({n[0], n[1], n[4], n[3]}); // quad
            out.push_back({n[1], n[2], n[5], n[4]}); // quad
            out.push_back({n[2], n[0], n[3], n[5]}); // quad
        }
        break;
    default:
        break;
    }
    return out;
}

[[nodiscard]] bool writeFile(const std::filesystem::path& path, const std::string& contents) {
    std::error_code ec;
    std::filesystem::create_directories(path.parent_path(), ec);
    if (ec)
        return false;
    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
        return false;
    out << contents;
    out.flush();
    return static_cast<bool>(out);
}

} // namespace

CompatibilityReport assessCompatibility(const FluentMesh& mesh, FluentFormat format,
                                        const std::string& filePath) {
    CompatibilityReport report;
    report.format = format;
    report.filePath = filePath;
    report.nodeCount = mesh.nodes.size();
    report.faceCount = mesh.faces.size();
    report.cellCount = mesh.cells.size();
    report.zoneCount = mesh.zones.size();

    // Detect unsupported cell types.
    std::size_t unsupportedCells = 0;
    for (const auto& c : mesh.cells)
        if (c.type == FluentCellType::Polyhedron || c.type == FluentCellType::Unknown)
            ++unsupportedCells;

    // Boundary zones → incompatibilities if they are not mappable.
    for (const auto& z : mesh.zones) {
        if (z.isBoundary && z.type != "wall" && z.type != "symmetry" &&
            z.type.find("inlet") == std::string::npos &&
            z.type.find("outlet") == std::string::npos &&
            z.type.find("interface") == std::string::npos) {
            report.incompatibilities.push_back(
                {"boundary zone '" + z.name + "'", z.type,
                 "no direct OpenFOAM patch-type mapping; will be created as 'patch'"});
        }
    }

    if (unsupportedCells > 0) {
        report.incompatibilities.push_back(
            {"cell topology", std::to_string(unsupportedCells) + " unsupported cells",
             "polyhedron/mixed cells are not converted by the MVP writer; "
             "use fluentMeshToFoam when available"});
    }

    // Decide level.
    if (mesh.empty()) {
        report.level = CompatibilityLevel::Incompatible;
    } else if (unsupportedCells == mesh.cells.size() && !mesh.cells.empty()) {
        report.level = CompatibilityLevel::VisualizationOnly;
    } else if (unsupportedCells > 0 || !report.incompatibilities.empty()) {
        report.level = CompatibilityLevel::PartiallyCompatible;
    } else {
        report.level = CompatibilityLevel::FullyCompatible;
    }

    if (mesh.dimension == 2)
        report.notes.push_back("2D mesh detected; OpenFOAM case will be 2D (empty front/back patches required).");
    report.notes.push_back("Boundary condition VALUES are not transferred: patch types are mapped, "
                          "values must be set by the user before running.");
    report.notes.push_back("No turbulence/physics model is inferred from the Fluent file.");

    return report;
}

Expected<std::vector<std::filesystem::path>>
convertToOpenFoam(const FluentMesh& mesh, const std::filesystem::path& caseDir,
                  CompatibilityReport* report) {
    if (mesh.empty())
        return Error::make(ErrorCode::InvalidData, "Fluent mesh is empty");

    // Node id → 0-based index.
    std::map<std::uint64_t, std::uint32_t> nodeIndex;
    std::uint32_t idx = 0;
    for (const auto& n : mesh.nodes)
        nodeIndex[n.id] = idx++;

    // Build face map: sorted node set → global face index.
    std::map<FaceKey, std::uint32_t> faceMap;
    std::vector<std::vector<std::uint32_t>> faces; // each face: list of point indices
    std::vector<std::int32_t> owner;
    std::vector<std::int32_t> neighbour;

    // Cells → owner assignment.
    std::vector<std::uint32_t> cellZoneOf; // not used beyond reporting
    std::uint32_t cellCount = 0;
    std::vector<std::uint32_t> cellTypesUnsupported;

    for (const auto& cell : mesh.cells) {
        if (cell.type == FluentCellType::Polyhedron || cell.type == FluentCellType::Unknown) {
            cellTypesUnsupported.push_back(static_cast<std::uint32_t>(cell.id));
            continue;
        }
        // Map node ids → point indices.
        std::vector<std::uint32_t> nodes;
        bool ok = true;
        for (auto nid : cell.nodeIds) {
            auto it = nodeIndex.find(nid);
            if (it == nodeIndex.end()) {
                ok = false;
                break;
            }
            nodes.push_back(it->second);
        }
        if (!ok || nodes.empty()) {
            cellTypesUnsupported.push_back(static_cast<std::uint32_t>(cell.id));
            continue;
        }
        auto cFaces = cellFaces(cell.type, nodes);
        for (const auto& cf : cFaces) {
            FaceKey key;
            key.nodes = cf;
            std::sort(key.nodes.begin(), key.nodes.end());
            auto it = faceMap.find(key);
            std::uint32_t faceIdx;
            if (it == faceMap.end()) {
                faceIdx = static_cast<std::uint32_t>(faces.size());
                faceMap[key] = faceIdx;
                // Store the (unsorted) node order for the face as found.
                faces.push_back(cf);
                owner.push_back(static_cast<std::int32_t>(cellCount));
                neighbour.push_back(-1);
            } else {
                faceIdx = it->second;
                if (neighbour[faceIdx] == -1 && owner[faceIdx] != static_cast<std::int32_t>(cellCount))
                    neighbour[faceIdx] = static_cast<std::int32_t>(cellCount);
            }
        }
        ++cellCount;
    }

    // Build boundary: group faces whose neighbour == -1 by their Fluent zone.
    // We don't have a direct face→zone map from the cell expansion, so fall
    // back to: a face is a boundary face if neighbour == -1. Assign all such
    // faces to a single "defaultFaces" patch unless zone info exists in
    // mesh.faces. Build a map from sorted node set → zone id using mesh.faces.
    std::map<FaceKey, std::uint32_t> faceZoneMap;
    for (const auto& f : mesh.faces) {
        FaceKey key;
        for (auto nid : f.nodeIds) {
            auto it = nodeIndex.find(nid);
            if (it != nodeIndex.end())
                key.nodes.push_back(it->second);
        }
        std::sort(key.nodes.begin(), key.nodes.end());
        faceZoneMap[key] = f.zoneId;
    }

    // Group boundary faces by zone.
    std::map<std::uint32_t, std::vector<std::uint32_t>> boundaryFacesByZone;
    for (std::size_t fi = 0; fi < faces.size(); ++fi) {
        if (neighbour[fi] != -1)
            continue;
        FaceKey key;
        key.nodes = faces[fi];
        std::sort(key.nodes.begin(), key.nodes.end());
        auto it = faceZoneMap.find(key);
        std::uint32_t zoneId = (it != faceZoneMap.end()) ? it->second : 0u;
        boundaryFacesByZone[zoneId].push_back(static_cast<std::uint32_t>(fi));
    }

    // Zone name lookup.
    std::map<std::uint32_t, FluentZone> zoneById;
    for (const auto& z : mesh.zones)
        zoneById[z.id] = z;

    // Write polyMesh files.
    std::error_code ec;
    const auto polyMesh = caseDir / "constant" / "polyMesh";
    std::filesystem::create_directories(polyMesh, ec);
    if (ec)
        return Error::make(ErrorCode::IoError, "cannot create polyMesh dir", ec.message());

    std::vector<std::filesystem::path> written;

    // points
    {
        std::ostringstream out;
        out << "/*--------------------------------*- C++ -*----------------------------------*\\\n"
            << "| Generated by Vortia from Fluent mesh (best-effort).                       |\n"
            << "\\*---------------------------------------------------------------------------*/\n"
            << "FoamFile\n{\n    version 2.0;\n    format ascii;\n    class vectorField;\n"
            << "    object points;\n}\n// * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * //\n\n"
            << mesh.nodes.size() << "\n(\n";
        for (const auto& n : mesh.nodes)
            out << "(" << n.position.x * mesh.scale << " " << n.position.y * mesh.scale << " "
                << n.position.z * mesh.scale << ")\n";
        out << ")\n\n// ************************************************************************* //\n";
        const auto p = polyMesh / "points";
        if (!writeFile(p, out.str()))
            return Error::make(ErrorCode::IoError, "cannot write points");
        written.push_back(p);
    }

    // faces
    {
        std::ostringstream out;
        out << "/* Vortia */\nFoamFile\n{\n    version 2.0;\n    format ascii;\n    class faceList;\n"
            << "    object faces;\n}\n\n" << faces.size() << "\n(\n";
        for (const auto& face : faces) {
            out << face.size();
            for (auto ni : face)
                out << " " << ni;
            out << "\n";
        }
        out << ")\n\n// ************************************************************************* //\n";
        const auto p = polyMesh / "faces";
        if (!writeFile(p, out.str()))
            return Error::make(ErrorCode::IoError, "cannot write faces");
        written.push_back(p);
    }

    // owner
    {
        std::ostringstream out;
        out << "FoamFile\n{\n    version 2.0;\n    format ascii;\n    class labelList;\n    object owner;\n}\n\n"
            << owner.size() << "\n(\n";
        for (auto o : owner)
            out << o << "\n";
        out << ")\n";
        const auto p = polyMesh / "owner";
        if (!writeFile(p, out.str()))
            return Error::make(ErrorCode::IoError, "cannot write owner");
        written.push_back(p);
    }

    // neighbour
    {
        std::ostringstream out;
        out << "FoamFile\n{\n    version 2.0;\n    format ascii;\n    class labelList;\n    object neighbour;\n}\n\n"
            << neighbour.size() << "\n(\n";
        std::size_t nInterior = 0;
        for (auto nb : neighbour) {
            if (nb >= 0) {
                out << nb << "\n";
                ++nInterior;
            }
        }
        out << ")\n";
        // OpenFOAM expects neighbour to contain only interior faces; the count
        // header must match. Rewrite header with the interior count.
        // (We already wrote the interior entries above; fix the count.)
        std::string contents = out.str();
        // Rebuild cleanly to be safe:
        std::ostringstream out2;
        out2 << "FoamFile\n{\n    version 2.0;\n    format ascii;\n    class labelList;\n    object neighbour;\n}\n\n"
             << nInterior << "\n(\n";
        for (auto nb : neighbour)
            if (nb >= 0)
                out2 << nb << "\n";
        out2 << ")\n";
        const auto p = polyMesh / "neighbour";
        if (!writeFile(p, out2.str()))
            return Error::make(ErrorCode::IoError, "cannot write neighbour");
        written.push_back(p);
    }

    // boundary
    {
        std::ostringstream out;
        out << "FoamFile\n{\n    version 2.0;\n    format ascii;\n    class polyBoundaryMesh;\n    object boundary;\n}\n\n"
            << boundaryFacesByZone.size() << "\n(\n";
        for (const auto& [zoneId, faceIds] : boundaryFacesByZone) {
            std::string name = "zone-" + std::to_string(zoneId);
            std::string type = "patch";
            auto it = zoneById.find(zoneId);
            if (it != zoneById.end()) {
                if (!it->second.name.empty() && it->second.name.find("zone-") != 0)
                    name = it->second.name;
                const std::string& t = it->second.type;
                if (t == "wall")
                    type = "wall";
                else if (t == "symmetry")
                    type = "symmetry";
                else if (t == "axis")
                    type = "empty";
                else
                    type = "patch";
            }
            if (zoneId == 0)
                name = "defaultFaces";
            out << "    " << name << "\n    {\n"
                << "        type " << type << ";\n"
                << "        nFaces " << faceIds.size() << ";\n"
                << "        startFace " << faceIds.front() << ";\n"
                << "    }\n";
        }
        out << ")\n\n// ************************************************************************* //\n";
        const auto p = polyMesh / "boundary";
        if (!writeFile(p, out.str()))
            return Error::make(ErrorCode::IoError, "cannot write boundary");
        written.push_back(p);
    }

    // Minimal controlDict so the case is recognisable.
    {
        std::ostringstream out;
        out << "FoamFile\n{\n    version 2.0;\n    format ascii;\n    class dictionary;\n    object controlDict;\n}\n\n"
            << "application simpleFoam;\nstartFrom startTime;\nstartTime 0;\nstopAt endTime;\n"
            << "endTime 1;\ndeltaT 1;\nwriteControl timeStep;\nwriteInterval 1;\n"
            << "purgeWrite 0;\nwriteFormat ascii;\nwritePrecision 6;\nwriteCompression off;\n"
            << "timeFormat general;\ntimePrecision 6;\nrunTimeModifiable true;\n";
        const auto p = caseDir / "system" / "controlDict";
        if (!writeFile(p, out.str()))
            return Error::make(ErrorCode::IoError, "cannot write controlDict");
        written.push_back(p);
    }

    if (report) {
        report->level = (cellTypesUnsupported.empty())
                            ? CompatibilityLevel::FullyCompatible
                            : CompatibilityLevel::PartiallyCompatible;
        if (!cellTypesUnsupported.empty()) {
            report->incompatibilities.push_back(
                {"cells", std::to_string(cellTypesUnsupported.size()) + " unsupported cells",
                 "polyhedron/mixed cells skipped by the MVP writer"});
        }
        report->notes.push_back("Mesh converted to OpenFOAM polyMesh (raw format). "
                                "Run checkMesh to validate.");
    }

    log(LogLevel::Info, "Converted Fluent mesh to OpenFOAM case: " +
                            std::to_string(mesh.nodes.size()) + " points, " +
                            std::to_string(faces.size()) + " faces, " +
                            std::to_string(cellCount) + " cells");

    return written;
}

} // namespace vortia::fluent
