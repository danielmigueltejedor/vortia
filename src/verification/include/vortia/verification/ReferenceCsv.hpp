#pragma once

#include "vortia/core/Error.hpp"
#include "vortia/core/Expected.hpp"

#include <algorithm>
#include <filesystem>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

namespace vortia::verification {

struct Sample1D {
    double x{0.0};
    double y{0.0};
};

/// Linear interpolation of y(x) onto query abscissae. Requires sorted unique x.
[[nodiscard]] inline Expected<std::vector<double>>
interpolateAt(const std::vector<Sample1D>& sorted, const std::vector<double>& queries) {
    if (sorted.size() < 2)
        return Error::make(ErrorCode::InvalidData, "need at least two samples to interpolate");
    for (std::size_t i = 1; i < sorted.size(); ++i) {
        if (!(sorted[i].x > sorted[i - 1].x))
            return Error::make(ErrorCode::InvalidData, "samples must be strictly increasing in x");
    }
    std::vector<double> out;
    out.reserve(queries.size());
    for (double q : queries) {
        if (q <= sorted.front().x) {
            out.push_back(sorted.front().y);
            continue;
        }
        if (q >= sorted.back().x) {
            out.push_back(sorted.back().y);
            continue;
        }
        const auto it = std::upper_bound(sorted.begin(), sorted.end(), q,
                                         [](double v, const Sample1D& s) { return v < s.x; });
        const auto hi = it;
        const auto lo = std::prev(it);
        const double t = (q - lo->x) / (hi->x - lo->x);
        out.push_back(lo->y + t * (hi->y - lo->y));
    }
    return out;
}

namespace detail {
[[nodiscard]] inline bool tryParseXy(const std::string& line, Sample1D* out) {
    std::string copy = line;
    std::replace(copy.begin(), copy.end(), ',', ' ');
    std::istringstream ss(copy);
    Sample1D s;
    if (!(ss >> s.x >> s.y))
        return false;
    if (out)
        *out = s;
    return true;
}
} // namespace detail

/// Load a two-column CSV (x,y). Lines starting with '#' and non-numeric headers are skipped.
[[nodiscard]] inline Expected<std::vector<Sample1D>> loadXyCsv(const std::filesystem::path& path) {
    std::ifstream in(path);
    if (!in)
        return Error::make(ErrorCode::IoError, "cannot open reference CSV", path.string());
    std::vector<Sample1D> rows;
    std::string line;
    while (std::getline(in, line)) {
        if (line.empty())
            continue;
        if (!line.empty() && line.back() == '\r')
            line.pop_back();
        if (line[0] == '#')
            continue;
        Sample1D s;
        if (!detail::tryParseXy(line, &s))
            continue; // header or junk
        rows.push_back(s);
    }
    if (rows.size() < 2)
        return Error::make(ErrorCode::InvalidData, "CSV has fewer than two data rows", path.string());
    std::sort(rows.begin(), rows.end(), [](const Sample1D& a, const Sample1D& b) { return a.x < b.x; });
    for (std::size_t i = 1; i < rows.size(); ++i) {
        if (!(rows[i].x > rows[i - 1].x))
            return Error::make(ErrorCode::InvalidData, "CSV abscissae are not unique", path.string());
    }
    return rows;
}

} // namespace vortia::verification
