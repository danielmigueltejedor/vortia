#include "vortia/interop/fluent/FluentFormatDetector.hpp"

#include <algorithm>
#include <cstdint>
#include <fstream>

namespace vortia::fluent {

namespace {

[[nodiscard]] std::vector<unsigned char> readMagic(const std::filesystem::path& path,
                                                   std::size_t n) {
    std::ifstream in(path, std::ios::binary);
    if (!in)
        return {};
    std::vector<unsigned char> bytes(n);
    in.read(reinterpret_cast<char*>(bytes.data()), static_cast<std::streamsize>(n));
    bytes.resize(static_cast<std::size_t>(in.gcount()));
    return bytes;
}

} // namespace

bool isHdf5(const std::filesystem::path& path) {
    const auto magic = readMagic(path, 8);
    // HDF5 superblock magic: 89 48 44 46 0d 0a 1a 0a
    return magic.size() >= 8 && magic[0] == 0x89 && magic[1] == 0x48 && magic[2] == 0x44 &&
           magic[3] == 0x46 && magic[4] == 0x0d && magic[5] == 0x0a && magic[6] == 0x1a &&
           magic[7] == 0x0a;
}

bool isGzip(const std::filesystem::path& path) {
    const auto magic = readMagic(path, 2);
    return magic.size() >= 2 && magic[0] == 0x1f && magic[1] == 0x8b;
}

std::string readHeader(const std::filesystem::path& path, std::size_t maxBytes) {
    std::ifstream in(path, std::ios::binary);
    if (!in)
        return {};
    std::string text(maxBytes, '\0');
    in.read(text.data(), static_cast<std::streamsize>(maxBytes));
    text.resize(static_cast<std::size_t>(in.gcount()));
    return text;
}

FluentFormat detectFormat(const std::filesystem::path& path) {
    const std::string name = path.filename().string();
    const std::string lower = [&]() {
        std::string s = name;
        std::transform(s.begin(), s.end(), s.begin(),
                       [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
        return s;
    }();

    if (isGzip(path))
        return FluentFormat::Compressed;

    if (isHdf5(path)) {
        if (lower.find(".msh.h5") != std::string::npos || lower.find(".mesh.h5") != std::string::npos)
            return FluentFormat::MeshHdf5;
        if (lower.find(".cas.h5") != std::string::npos || lower.find(".case.h5") != std::string::npos)
            return FluentFormat::CaseHdf5;
        if (lower.find(".dat.h5") != std::string::npos || lower.find(".data.h5") != std::string::npos)
            return FluentFormat::DataHdf5;
        // HDF5 without a recognised suffix: inspect the root group names.
        // CFF files store a "/mesh" or "/case" group; fall back to Unknown.
        return FluentFormat::Unknown;
    }

    // ASCII path: identify by content markers.
    const std::string header = readHeader(path, 8192);
    const bool hasGrid = header.find("\"Grid\"") != std::string::npos ||
                         header.find("(0 \"Grid") != std::string::npos ||
                         header.find("Grid:") != std::string::npos;
    const bool hasCase = header.find("\"Case\"") != std::string::npos ||
                         header.find("(0 \"Case") != std::string::npos;
    const bool hasData = header.find("\"Data\"") != std::string::npos ||
                         header.find("(0 \"Data") != std::string::npos;

    if (lower.find(".msh") != std::string::npos && (hasGrid || !header.empty()))
        return FluentFormat::MeshAscii;
    if (lower.find(".cas") != std::string::npos && (hasCase || !header.empty()))
        return FluentFormat::CaseAscii;
    if (lower.find(".dat") != std::string::npos && (hasData || !header.empty()))
        return FluentFormat::DataAscii;

    // Extension-less fallback by content.
    if (hasGrid)
        return FluentFormat::MeshAscii;
    if (hasCase)
        return FluentFormat::CaseAscii;
    if (hasData)
        return FluentFormat::DataAscii;

    if (lower.find(".jou") != std::string::npos)
        return FluentFormat::Journal;
    if (lower.find(".flprj") != std::string::npos || lower.find(".wbpj") != std::string::npos ||
        lower.find(".wbpz") != std::string::npos)
        return FluentFormat::WorkbenchProject;

    return FluentFormat::Unknown;
}

std::string_view toString(FluentFormat f) {
    switch (f) {
    case FluentFormat::Unknown: return "unknown";
    case FluentFormat::MeshAscii: return "mesh-ascii";
    case FluentFormat::MeshBinary: return "mesh-binary";
    case FluentFormat::MeshHdf5: return "mesh-hdf5 (CFF)";
    case FluentFormat::CaseAscii: return "case-ascii";
    case FluentFormat::CaseHdf5: return "case-hdf5 (CFF)";
    case FluentFormat::DataAscii: return "data-ascii";
    case FluentFormat::DataHdf5: return "data-hdf5 (CFF)";
    case FluentFormat::Compressed: return "compressed";
    case FluentFormat::Journal: return "journal";
    case FluentFormat::WorkbenchProject: return "workbench-project";
    }
    return "unknown";
}

std::string_view toString(CompatibilityLevel level) {
    switch (level) {
    case CompatibilityLevel::FullyCompatible: return "fully-compatible";
    case CompatibilityLevel::PartiallyCompatible: return "partially-compatible";
    case CompatibilityLevel::VisualizationOnly: return "visualization-only";
    case CompatibilityLevel::NeedsConfiguration: return "needs-configuration";
    case CompatibilityLevel::Incompatible: return "incompatible";
    }
    return "incompatible";
}

} // namespace vortia::fluent
