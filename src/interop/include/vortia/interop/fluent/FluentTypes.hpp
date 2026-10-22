#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/core/Vec3.hpp"

#include <cstdint>
#include <filesystem>
#include <map>
#include <string>
#include <vector>

namespace vortia::fluent {

/// Fluent file families. Identification is by magic bytes + structure, not
/// just the extension (a renamed file must still be recognised).
enum class FluentFormat {
    Unknown,
    MeshAscii,       ///< Traditional .msh (ASCII)
    MeshBinary,      ///< Traditional .msh (binary, rare)
    MeshHdf5,        ///< .msh.h5 (CFF)
    CaseAscii,       ///< Traditional .cas
    CaseHdf5,        ///< .cas.h5 (CFF)
    DataAscii,       ///< Traditional .dat
    DataHdf5,        ///< .dat.h5 (CFF)
    Compressed,      ///< gzip-wrapped variant of any of the above
    Journal,         ///< .jou (P2)
    WorkbenchProject ///< .flprj/.wbpj/.wbpz (P2)
};

[[nodiscard]] std::string_view toString(FluentFormat f);

/// How Vortia can use the file.
enum class CompatibilityLevel {
    FullyCompatible,     ///< Can convert to a runnable OpenFOAM case
    PartiallyCompatible, ///< Mesh/BCs imported, some physics lost
    VisualizationOnly,   ///< Can display but not simulate
    NeedsConfiguration,  ///< Imported, user must fill gaps before running
    Incompatible         ///< Cannot be used
};

[[nodiscard]] std::string_view toString(CompatibilityLevel level);

// ---------------------------------------------------------------------------
// Intermediate data model: a Fluent mesh decoupled from any file format.
// ---------------------------------------------------------------------------

/// A node (vertex) in the Fluent mesh.
struct FluentNode {
    std::uint64_t id{0};
    Vec3 position{};
};

/// A face: ordered node indices + zone id. Zone type determines boundary vs
/// interior.
struct FluentFace {
    std::uint64_t id{0};
    std::vector<std::uint32_t> nodeIds; ///< 1-based Fluent node ids
    std::uint32_t zoneId{0};
    bool isBoundary{false};
};

/// A cell: type + node indices + zone id.
enum class FluentCellType { Tetra, Hexa, Pyramid, Wedge, Prism, Polyhedron, Unknown };
struct FluentCell {
    std::uint64_t id{0};
    FluentCellType type{FluentCellType::Unknown};
    std::vector<std::uint32_t> nodeIds;
    std::uint32_t zoneId{0};
};

/// A zone (region) in the Fluent mesh.
struct FluentZone {
    std::uint32_t id{0};
    std::string name;
    std::string type; ///< "interior", "wall", "velocity-inlet", "pressure-outlet", ...
    bool isBoundary{false};
};

/// The mesh-level intermediate representation.
struct FluentMesh {
    int dimension{3}; ///< 2 or 3
    double scale{1.0}; ///< unit scale to metres (1.0 if already SI)
    std::string unitsDescription{"metres"};
    std::vector<FluentNode> nodes;
    std::vector<FluentFace> faces;
    std::vector<FluentCell> cells;
    std::vector<FluentZone> zones;
    std::map<std::uint32_t, std::string> cellZoneNames;

    [[nodiscard]] bool empty() const { return nodes.empty() && cells.empty(); }
};

/// A property that could not be translated to OpenFOAM.
struct Incompatibility {
    std::string component; ///< "turbulence model", "BC: inlet", ...
    std::string fluentValue;
    std::string reason;
};

/// Compatibility report produced by every import.
struct CompatibilityReport {
    FluentFormat format{FluentFormat::Unknown};
    std::string filePath;
    std::string version;       ///< Fluent version string if detected
    std::size_t nodeCount{0};
    std::size_t faceCount{0};
    std::size_t cellCount{0};
    std::size_t zoneCount{0};
    CompatibilityLevel level{CompatibilityLevel::Incompatible};
    std::vector<std::string> warnings;
    std::vector<Incompatibility> incompatibilities;
    std::vector<std::string> notes; ///< free-form guidance for the user
};

} // namespace vortia::fluent
