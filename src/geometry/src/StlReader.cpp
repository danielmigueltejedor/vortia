#include "vortia/geometry/StlReader.hpp"

#include <locale>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <sstream>
#include <vector>

namespace vortia {

namespace {

constexpr std::size_t kBinaryHeaderSize = 80;
constexpr std::size_t kBinaryFacetSize = 50; // 12 floats + uint16 attribute

[[nodiscard]] bool isFinite(const Vec3& v) {
    return std::isfinite(v.x) && std::isfinite(v.y) && std::isfinite(v.z);
}

Expected<TriangleMesh> parseBinary(const std::vector<char>& bytes,
                                   const std::filesystem::path& path) {
    if (bytes.size() < kBinaryHeaderSize + sizeof(std::uint32_t))
        return Error::make(ErrorCode::InvalidData, "binary STL shorter than header",
                           path.string());

    std::uint32_t count = 0;
    std::memcpy(&count, bytes.data() + kBinaryHeaderSize, sizeof(count));
    const std::uint64_t expected = kBinaryHeaderSize + sizeof(std::uint32_t) +
                                   static_cast<std::uint64_t>(count) * kBinaryFacetSize;
    if (expected != bytes.size())
        return Error::make(ErrorCode::InvalidData,
                           "binary STL size does not match triangle count " +
                               std::to_string(count),
                           path.string());

    TriangleMesh mesh;
    mesh.reserve(static_cast<std::size_t>(count) * 3u, count);
    const char* cursor = bytes.data() + kBinaryHeaderSize + sizeof(std::uint32_t);
    for (std::uint32_t i = 0; i < count; ++i) {
        float values[12]; // normal(3) + v1(3) + v2(3) + v3(3)
        std::memcpy(values, cursor, sizeof(values));
        cursor += kBinaryFacetSize;
        for (int corner = 0; corner < 3; ++corner) {
            const Vec3 v{static_cast<double>(values[3 + corner * 3]),
                         static_cast<double>(values[4 + corner * 3]),
                         static_cast<double>(values[5 + corner * 3])};
            if (!isFinite(v))
                return Error::make(ErrorCode::InvalidData,
                                   "non-finite vertex in triangle " + std::to_string(i),
                                   path.string());
            mesh.addVertex(v);
        }
        const auto base = static_cast<std::uint32_t>(mesh.vertices().size()) - 3u;
        mesh.addTriangle({base, base + 1u, base + 2u});
    }
    return mesh;
}

/// ASCII STL tokenizer: the grammar is line/word oriented; we accept any
/// whitespace mixture but require the exact keyword sequence.
class AsciiParser {
public:
    explicit AsciiParser(std::string_view text) : text_(text) {}

    Expected<std::string> next() {
        skipWhitespace();
        if (pos_ >= text_.size())
            return Error::make(ErrorCode::ParseError, "unexpected end of ASCII STL");
        const std::size_t begin = pos_;
        while (pos_ < text_.size() && !isSpace(text_[pos_]))
            ++pos_;
        ++wordCount_;
        return std::string(text_.substr(begin, pos_ - begin));
    }

    Expected<double> nextNumber() {
        auto word = next();
        if (!word)
            return word.error();
        // macOS libc++ on GitHub's macos-15 runner lacks floating
        // std::from_chars. A classic-locale stream keeps STL's '.' decimals
        // independent of the user's UI language (including Spanish locales).
        std::istringstream stream(word.value());
        stream.imbue(std::locale::classic());
        double value = 0.0;
        if (!(stream >> value))
            return Error::make(ErrorCode::ParseError, "invalid number '" + word.value() + "'");
        stream >> std::ws;
        if (!stream.eof() || !std::isfinite(value))
            return Error::make(ErrorCode::ParseError, "invalid number '" + word.value() + "'");
        return value;
    }

    Expected<void> expect(std::string_view keyword) {
        auto word = next();
        if (!word)
            return Error::make(ErrorCode::ParseError,
                               "expected '" + std::string(keyword) + "', got end of file");
        if (word.value() != keyword)
            return Error::make(ErrorCode::ParseError,
                               "expected '" + std::string(keyword) + "', got '" + word.value() +
                                   "'");
        return {};
    }

    [[nodiscard]] bool atEnd() {
        skipWhitespace();
        return pos_ >= text_.size();
    }

private:
    static bool isSpace(char c) {
        return c == ' ' || c == '\t' || c == '\r' || c == '\n' || c == '\f' || c == '\v';
    }
    void skipWhitespace() {
        while (pos_ < text_.size() && isSpace(text_[pos_]))
            ++pos_;
    }

    std::string_view text_;
    std::size_t pos_{0};
    std::size_t wordCount_{0};
};

Expected<TriangleMesh> parseAscii(std::string_view text, const std::filesystem::path& path) {
    AsciiParser parser(text);
    if (auto e = parser.expect("solid"); !e)
        return e.error();

    // The solid name is free text until the next known keyword; consume it
    // word by word until "facet" or "endsolid".
    TriangleMesh mesh;
    std::size_t triangleIndex = 0;
    for (;;) {
        auto word = parser.next();
        if (!word)
            return Error::make(ErrorCode::ParseError, "ASCII STL ends inside solid",
                               path.string());
        if (word.value() == "endsolid") {
            // Optional trailing solid name; ignore everything after.
            return mesh;
        }
        if (word.value() != "facet")
            continue; // still inside the solid name

        if (auto e = parser.expect("normal"); !e)
            return e.error().code == ErrorCode::ParseError
                        ? Error::make(ErrorCode::ParseError,
                                      "triangle " + std::to_string(triangleIndex) + ": " +
                                          e.error().message,
                                      path.string())
                        : e.error();
        for (int i = 0; i < 3; ++i) // normal components (not stored)
            if (auto n = parser.nextNumber(); !n)
                return n.error();
        if (auto e = parser.expect("outer"); !e)
            return e.error();
        if (auto e = parser.expect("loop"); !e)
            return e.error();

        const auto base = static_cast<std::uint32_t>(mesh.vertices().size());
        for (int corner = 0; corner < 3; ++corner) {
            if (auto e = parser.expect("vertex"); !e)
                return e.error();
            Vec3 v;
            for (int i = 0; i < 3; ++i) {
                auto n = parser.nextNumber();
                if (!n)
                    return Error::make(ErrorCode::ParseError,
                                       "triangle " + std::to_string(triangleIndex) + ": " +
                                           n.error().message,
                                       path.string());
                v[i] = n.value();
            }
            if (!isFinite(v))
                return Error::make(ErrorCode::InvalidData,
                                   "non-finite vertex in triangle " +
                                       std::to_string(triangleIndex),
                                   path.string());
            mesh.addVertex(v);
        }
        mesh.addTriangle({base, base + 1u, base + 2u});
        ++triangleIndex;

        if (auto e = parser.expect("endloop"); !e)
            return e.error();
        if (auto e = parser.expect("endfacet"); !e)
            return e.error();
    }
}

} // namespace

Expected<TriangleMesh> StlReader::read(const std::filesystem::path& path) {
    std::error_code ec;
    if (!std::filesystem::is_regular_file(path, ec))
        return Error::make(ErrorCode::NotFound, "STL file does not exist", path.string());
    const auto size = std::filesystem::file_size(path, ec);
    if (ec)
        return Error::make(ErrorCode::IoError, "cannot stat STL file", ec.message());
    if (size == 0)
        return Error::make(ErrorCode::InvalidData, "STL file is empty", path.string());
    if (size > kMaxFileSize)
        return Error::make(ErrorCode::InvalidData, "STL file exceeds the 2 GiB safety limit",
                           path.string());

    std::ifstream in(path, std::ios::binary);
    if (!in)
        return Error::make(ErrorCode::PermissionDenied, "cannot open STL file", path.string());
    std::vector<char> bytes(static_cast<std::size_t>(size));
    in.read(bytes.data(), static_cast<std::streamsize>(bytes.size()));
    if (in.gcount() != static_cast<std::streamsize>(bytes.size()))
        return Error::make(ErrorCode::IoError, "short read on STL file", path.string());

    // Format detection: exact binary size match wins even over a "solid" header.
    bool binary = false;
    if (bytes.size() >= kBinaryHeaderSize + sizeof(std::uint32_t)) {
        std::uint32_t count = 0;
        std::memcpy(&count, bytes.data() + kBinaryHeaderSize, sizeof(count));
        binary = (kBinaryHeaderSize + sizeof(std::uint32_t) +
                  static_cast<std::uint64_t>(count) * kBinaryFacetSize) == bytes.size();
    }

    Expected<TriangleMesh> mesh = binary
        ? parseBinary(bytes, path)
        : parseAscii(std::string_view(bytes.data(), bytes.size()), path);
    if (!mesh)
        return mesh.error();
    if (mesh->isEmpty())
        return Error::make(ErrorCode::InvalidData, "STL contains no triangles", path.string());
    mesh->setSourceDescription(path.filename().string() + (binary ? " (binary)" : " (ASCII)"));
    return mesh;
}

} // namespace vortia
