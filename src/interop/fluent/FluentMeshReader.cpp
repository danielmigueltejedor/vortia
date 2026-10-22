#include "vortia/interop/fluent/FluentFormatDetector.hpp"
#include "vortia/interop/fluent/FluentMeshReader.hpp"

#include "vortia/core/Logger.hpp"

#include <algorithm>
#include <cctype>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

namespace vortia::fluent {

namespace {

// ---------------------------------------------------------------------------
// Minimal S-expression tokenizer + parser for Fluent .msh (ASCII).
// ---------------------------------------------------------------------------

struct Token {
    enum Type { LParen, RParen, String, Number, Symbol } type{Symbol};
    std::string text;
};

std::vector<Token> tokenize(const std::string& input) {
    std::vector<Token> tokens;
    std::size_t i = 0;
    const std::size_t n = input.size();
    while (i < n) {
        const char c = input[i];
        if (std::isspace(static_cast<unsigned char>(c))) {
            ++i;
            continue;
        }
        if (c == '(') {
            tokens.push_back({Token::LParen, "("});
            ++i;
        } else if (c == ')') {
            tokens.push_back({Token::RParen, ")"});
            ++i;
        } else if (c == '"') {
            std::string s;
            ++i;
            while (i < n && input[i] != '"') {
                if (input[i] == '\\' && i + 1 < n) {
                    s += input[i + 1];
                    i += 2;
                } else {
                    s += input[i++];
                }
            }
            ++i; // closing quote
            tokens.push_back({Token::String, s});
        } else {
            // Number or symbol: read until whitespace or paren.
            std::size_t start = i;
            while (i < n && !std::isspace(static_cast<unsigned char>(input[i])) &&
                   input[i] != '(' && input[i] != ')')
                ++i;
            Token t;
            t.text = input.substr(start, i - start);
            // Classify as number if it parses.
            bool isNum = !t.text.empty();
            for (char ch : t.text)
                if (!(std::isdigit(static_cast<unsigned char>(ch)) || ch == '.' || ch == '-' ||
                      ch == '+' || ch == 'e' || ch == 'E'))
                    isNum = false;
            t.type = isNum ? Token::Number : Token::Symbol;
            tokens.push_back(t);
        }
    }
    return tokens;
}

struct SExpr {
    enum Type { List, Atom } type{Atom};
    std::string atom;
    std::vector<SExpr> children;
};

// Recursive descent: returns one SExpr and advances index.
SExpr parseOne(const std::vector<Token>& tokens, std::size_t& idx) {
    SExpr expr;
    if (idx >= tokens.size())
        return expr;
    if (tokens[idx].type == Token::LParen) {
        expr.type = SExpr::List;
        ++idx;
        while (idx < tokens.size() && tokens[idx].type != Token::RParen) {
            expr.children.push_back(parseOne(tokens, idx));
        }
        if (idx < tokens.size())
            ++idx; // consume ')'
    } else {
        expr.type = SExpr::Atom;
        expr.atom = tokens[idx].text;
        ++idx;
    }
    return expr;
}

std::vector<SExpr> parseAll(const std::vector<Token>& tokens) {
    std::vector<SExpr> out;
    std::size_t idx = 0;
    while (idx < tokens.size())
        out.push_back(parseOne(tokens, idx));
    return out;
}

// Helpers to read atoms.
bool isNum(const SExpr& e) { return e.type == SExpr::Atom; }
long long asInt(const SExpr& e) {
    try {
        return std::stoll(e.atom);
    } catch (...) {
        return 0;
    }
}
double asDouble(const SExpr& e) {
    try {
        return std::stod(e.atom);
    } catch (...) {
        return 0.0;
    }
}

// Map Fluent zone type numbers (section 14) to names.
std::string zoneTypeName(int t) {
    switch (t) {
    case 0: return "dead";
    case 1: return "axis";
    case 2: return "interior";
    case 3: return "wall";
    case 4: return "pressure-far-field";
    case 5: return "velocity-inlet";
    case 6: return "pressure-inlet";
    case 7: return "pressure-outlet";
    case 8: return "symmetry";
    case 9: return "periodic-shadow";
    case 10: return "periodic";
    case 11: return "interface";
    case 12: return "outflow";
    case 14: return "mass-flow-inlet";
    case 20: return "porous-jump";
    case 24: return "fan";
    default: return "unknown-" + std::to_string(t);
    }
}

FluentCellType cellTypeFromFluent(int t) {
    switch (t) {
    case 0: return FluentCellType::Unknown;
    case 1: return FluentCellType::Unknown; // mixed
    case 2: return FluentCellType::Unknown; // dead
    case 3: return FluentCellType::Tetra;
    case 4: return FluentCellType::Hexa;
    case 5: return FluentCellType::Pyramid;
    case 6: return FluentCellType::Wedge;
    case 7: return FluentCellType::Polyhedron;
    default: return FluentCellType::Unknown;
    }
}

} // namespace

Expected<FluentMesh> readTraditionalMesh(const std::filesystem::path& path) {
    std::ifstream in(path);
    if (!in)
        return Error::make(ErrorCode::NotFound, "cannot open Fluent mesh file", path.string());

    std::ostringstream ss;
    ss << in.rdbuf();
    const std::string text = ss.str();

    if (text.find("Grid") == std::string::npos && text.find("(") == std::string::npos)
        return Error::make(ErrorCode::InvalidData, "file does not look like a Fluent .msh",
                           path.string());

    const auto tokens = tokenize(text);
    const auto sections = parseAll(tokens);

    FluentMesh mesh;
    std::map<std::uint32_t, FluentZone> zonesById;

    for (const auto& section : sections) {
        if (section.type != SExpr::List || section.children.empty())
            continue;
        if (section.children[0].type != SExpr::Atom)
            continue;
        const int sectionType = [&]() {
            try {
                return std::stoi(section.children[0].atom);
            } catch (...) {
                return -1;
            }
        }();
        if (sectionType < 0)
            continue;

        if (sectionType == 10) {
            // Nodes. First child is the zone-id list.
            // Subsequent children are node records: (id x y [z]).
            for (std::size_t ci = 1; ci < section.children.size(); ++ci) {
                const auto& rec = section.children[ci];
                if (rec.type != SExpr::List || rec.children.size() < 3)
                    continue;
                FluentNode node;
                node.id = static_cast<std::uint64_t>(asInt(rec.children[0]));
                node.position.x = asDouble(rec.children[1]);
                node.position.y = asDouble(rec.children[2]);
                if (rec.children.size() >= 4)
                    node.position.z = asDouble(rec.children[3]);
                else
                    node.position.z = 0.0;
                mesh.nodes.push_back(node);
            }
        } else if (sectionType == 12) {
            // Faces. Records: (id n0 n1 ... zoneL zoneR).
            for (std::size_t ci = 1; ci < section.children.size(); ++ci) {
                const auto& rec = section.children[ci];
                if (rec.type != SExpr::List || rec.children.size() < 4)
                    continue;
                // Skip zone-id header lists (they have 1-2 elements and no
                // trailing zone pair? Heuristic: a face record has >= 4 numbers).
                // We only accept records whose last two entries parse as ints
                // and which have at least 2 node ids.
                FluentFace face;
                face.id = static_cast<std::uint64_t>(asInt(rec.children[0]));
                const std::size_t total = rec.children.size();
                // zoneL, zoneR are the last two
                const int zoneL = static_cast<int>(asInt(rec.children[total - 2]));
                const int zoneR = static_cast<int>(asInt(rec.children[total - 1]));
                face.zoneId = (zoneL != 0) ? static_cast<std::uint32_t>(zoneL)
                                           : static_cast<std::uint32_t>(zoneR);
                // A face is a boundary if one of the zones is 0.
                face.isBoundary = (zoneL == 0 || zoneR == 0);
                for (std::size_t k = 1; k + 2 <= total; ++k)
                    face.nodeIds.push_back(static_cast<std::uint32_t>(asInt(rec.children[k])));
                if (face.nodeIds.size() >= 2)
                    mesh.faces.push_back(face);
            }
        } else if (sectionType == 13) {
            // Cells. First child is the zone header: (zone_id cell_type ...).
            // Subsequent children are cell records: (cell_id node0 ...).
            int cellType = 0;
            if (section.children.size() > 1 && section.children[1].type == SExpr::List &&
                section.children[1].children.size() >= 2) {
                cellType = static_cast<int>(asInt(section.children[1].children[1]));
            }
            for (std::size_t ci = 2; ci < section.children.size(); ++ci) {
                const auto& rec = section.children[ci];
                if (rec.type != SExpr::List || rec.children.empty())
                    continue;
                FluentCell cell;
                cell.id = static_cast<std::uint64_t>(asInt(rec.children[0]));
                cell.type = cellTypeFromFluent(cellType);
                for (std::size_t k = 1; k < rec.children.size(); ++k)
                    cell.nodeIds.push_back(static_cast<std::uint32_t>(asInt(rec.children[k])));
                mesh.cells.push_back(cell);
            }
        } else if (sectionType == 14) {
            // Boundary zone types. Records: (zone_id type ...).
            for (std::size_t ci = 1; ci < section.children.size(); ++ci) {
                const auto& rec = section.children[ci];
                if (rec.type != SExpr::List || rec.children.size() < 2)
                    continue;
                FluentZone zone;
                zone.id = static_cast<std::uint32_t>(asInt(rec.children[0]));
                const int typeNum = static_cast<int>(asInt(rec.children[1]));
                zone.type = zoneTypeName(typeNum);
                zone.isBoundary = (typeNum != 2); // not interior
                if (rec.children.size() >= 3 && rec.children[2].type == SExpr::Atom)
                    zone.name = rec.children[2].atom;
                else
                    zone.name = "zone-" + std::to_string(zone.id);
                zonesById[zone.id] = zone;
            }
        } else if (sectionType == 2) {
            // Zone min/max ranges — store names if present.
            for (std::size_t ci = 1; ci < section.children.size(); ++ci) {
                const auto& rec = section.children[ci];
                if (rec.type != SExpr::List || rec.children.empty())
                    continue;
                const std::uint32_t id = static_cast<std::uint32_t>(asInt(rec.children[0]));
                if (zonesById.find(id) == zonesById.end()) {
                    FluentZone z;
                    z.id = id;
                    z.name = "zone-" + std::to_string(id);
                    z.type = "interior";
                    zonesById[id] = z;
                }
                if (rec.children.size() >= 2 && rec.children[1].type == SExpr::Atom)
                    zonesById[id].name = rec.children[1].atom;
            }
        }
    }

    // Assign zones to mesh.zones
    for (const auto& [id, zone] : zonesById)
        mesh.zones.push_back(zone);

    // Tag cells with zone ids is not available from section 13 header alone in
    // all files; leave zoneId 0 and let the report note it.

    // Tag faces with zone names/types is already in zonesById; the face.zoneId
    // already points at a zone id. Good.

    // Basic integrity checks.
    if (mesh.nodes.empty())
        return Error::make(ErrorCode::InvalidData, "Fluent mesh contains no nodes",
                           path.string());
    if (mesh.cells.empty() && mesh.faces.empty())
        return Error::make(ErrorCode::InvalidData, "Fluent mesh contains no cells or faces",
                           path.string());

    // Detect 2D: all z ~ 0
    bool allZZero = !mesh.nodes.empty();
    for (const auto& n : mesh.nodes)
        if (std::abs(n.position.z) > 1e-12) {
            allZZero = false;
            break;
        }
    mesh.dimension = allZZero ? 2 : 3;

    log(LogLevel::Info, "Imported Fluent mesh: " + std::to_string(mesh.nodes.size()) +
                            " nodes, " + std::to_string(mesh.faces.size()) + " faces, " +
                            std::to_string(mesh.cells.size()) + " cells, " +
                            std::to_string(mesh.zones.size()) + " zones (" +
                            (mesh.dimension == 2 ? "2D" : "3D") + ")");

    return mesh;
}

Expected<FluentMesh> readMesh(const std::filesystem::path& path) {
    const auto format = detectFormat(path);
    if (format == FluentFormat::MeshAscii)
        return readTraditionalMesh(path);
    if (format == FluentFormat::MeshHdf5)
        return Error::make(ErrorCode::UnsupportedFeature,
                           "CFF .msh.h5 mesh import requires VTK (planned P1); "
                           "open it via File → Import → ANSYS Fluent for visualization",
                           path.string());
    if (format == FluentFormat::Compressed)
        return Error::make(ErrorCode::UnsupportedFeature,
                           "Compressed Fluent meshes: decompress first or use the CFF reader",
                           path.string());
    return Error::make(ErrorCode::InvalidData, "not a recognised Fluent mesh file",
                       path.string());
}

} // namespace vortia::fluent
