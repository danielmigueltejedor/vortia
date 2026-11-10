#include "vortia/solver/openfoam/Forces.hpp"

#include <algorithm>
#include <charconv>
#include <cmath>
#include <sstream>

namespace vortia::openfoam {

namespace {

bool isOpenFoamWord(std::string_view name) {
    if (name.empty() || name.size() > 128)
        return false;
    return std::all_of(name.begin(), name.end(), [](unsigned char c) {
        return std::isalnum(c) || c == '_' || c == '-' || c == '.' || c == ':';
    });
}

std::string number(double value) {
    std::ostringstream out;
    out.precision(12);
    out << value;
    return out.str();
}

/// Numbers of a data row once brackets are treated as blanks.
bool readNumbers(std::string_view line, std::vector<double>& values) {
    values.clear();
    std::string cleaned(line);
    std::replace_if(cleaned.begin(), cleaned.end(),
                    [](char c) { return c == '(' || c == ')' || c == '\t' || c == ','; }, ' ');
    std::istringstream stream(cleaned);
    std::string token;
    while (stream >> token) {
        double value = 0.0;
        try {
            std::size_t used = 0;
            value = std::stod(token, &used);
            if (used != token.size())
                return false;
        } catch (const std::exception&) {
            return false;
        }
        if (!std::isfinite(value))
            return false;
        values.push_back(value);
    }
    return !values.empty();
}

Vec3 triple(const std::vector<double>& values, std::size_t first) {
    return {values[first], values[first + 1], values[first + 2]};
}

} // namespace

Expected<std::string> forcesFunctionObject(const ForcesRequest& request) {
    if (request.patches.empty())
        return Error::make(ErrorCode::InvalidArgument, "forces need at least one wall patch");
    for (const auto& patch : request.patches)
        if (!isOpenFoamWord(patch))
            return Error::make(ErrorCode::InvalidArgument, "invalid patch name for forces", patch);
    if (!std::isfinite(request.rhoInf) || request.rhoInf <= 0.0)
        return Error::make(ErrorCode::InvalidArgument, "reference density must be positive");
    for (std::size_t i = 0; i < 3; ++i)
        if (!std::isfinite(request.centreOfRotation[i]))
            return Error::make(ErrorCode::InvalidArgument, "centre of rotation must be finite");

    std::ostringstream out;
    out << "FoamFile\n{\n    version     2.0;\n    format      ascii;\n"
           "    class       dictionary;\n    object      forces;\n}\n\n"
        << "type            forces;\n"
        << "libs            (forces);\n"
        << "writeControl    writeTime;\n"
        << "patches         (";
    for (std::size_t i = 0; i < request.patches.size(); ++i)
        out << (i ? " " : "") << request.patches[i];
    out << ");\n"
        << "rho             rhoInf;\n"
        << "rhoInf          " << number(request.rhoInf) << ";\n"
        << "CofR            (" << number(request.centreOfRotation.x) << ' '
        << number(request.centreOfRotation.y) << ' ' << number(request.centreOfRotation.z)
        << ");\n";
    return out.str();
}

Expected<std::vector<ForceSample>> parseForces(std::string_view text) {
    std::vector<ForceSample> samples;
    std::vector<double> values;
    bool modernHeader = false;
    std::size_t lineNumber = 0;
    std::size_t begin = 0;
    while (begin <= text.size()) {
        const std::size_t end = std::min(text.find('\n', begin), text.size());
        std::string_view line = text.substr(begin, end - begin);
        begin = end + 1;
        ++lineNumber;
        while (!line.empty() && (line.back() == '\r' || line.back() == ' ' || line.back() == '\t'))
            line.remove_suffix(1);
        while (!line.empty() && (line.front() == ' ' || line.front() == '\t'))
            line.remove_prefix(1);
        if (line.empty())
            continue;
        if (line.front() == '#') {
            if (line.find("total_x") != std::string_view::npos)
                modernHeader = true;
            continue;
        }
        if (!readNumbers(line, values))
            return Error::make(ErrorCode::ParseError, "invalid number in forces file",
                               "line " + std::to_string(lineNumber));
        ForceSample sample;
        sample.time = values[0];
        if (modernHeader) {
            // Time total(3) pressure(3) viscous(3) [porous(3)]
            if (values.size() < 10)
                return Error::make(ErrorCode::ParseError, "forces row has too few columns",
                                   "line " + std::to_string(lineNumber));
            sample.total = triple(values, 1);
            sample.pressure = triple(values, 4);
            sample.viscous = triple(values, 7);
        } else {
            // Time ((pressure) (viscous) [(porous)]) ((moments) ...)
            if (values.size() < 7)
                return Error::make(ErrorCode::ParseError, "forces row has too few columns",
                                   "line " + std::to_string(lineNumber));
            sample.pressure = triple(values, 1);
            sample.viscous = triple(values, 4);
            sample.total = sample.pressure + sample.viscous;
        }
        samples.push_back(sample);
    }
    if (samples.empty())
        return Error::make(ErrorCode::ParseError, "forces file contains no data rows");
    return samples;
}

Expected<double> dynamicPressure(const FreeStream& stream) {
    if (!std::isfinite(stream.density) || stream.density <= 0.0)
        return Error::make(ErrorCode::InvalidArgument, "free-stream density must be positive");
    if (!std::isfinite(stream.speed) || stream.speed <= 0.0)
        return Error::make(ErrorCode::InvalidArgument, "free-stream speed must be positive");
    return 0.5 * stream.density * stream.speed * stream.speed;
}

Expected<ForceCoefficients>
forceCoefficients(const Vec3& force, const Vec3& dragDirection, const Vec3& liftDirection,
                  const FreeStream& stream) {
    const auto q = dynamicPressure(stream);
    if (!q)
        return q.error();
    if (!std::isfinite(stream.referenceArea) || stream.referenceArea <= 0.0)
        return Error::make(ErrorCode::InvalidArgument, "reference area must be positive");
    const double dragNorm = dragDirection.norm();
    const double liftNorm = liftDirection.norm();
    if (!std::isfinite(dragNorm) || !std::isfinite(liftNorm) || dragNorm <= 0.0 || liftNorm <= 0.0)
        return Error::make(ErrorCode::InvalidArgument, "drag and lift directions must be non-zero");
    const Vec3 drag = dragDirection / dragNorm;
    const Vec3 lift = liftDirection / liftNorm;
    if (std::abs(drag.dot(lift)) > 1e-9)
        return Error::make(ErrorCode::InvalidArgument,
                           "drag and lift directions must be perpendicular");
    for (std::size_t i = 0; i < 3; ++i)
        if (!std::isfinite(force[i]))
            return Error::make(ErrorCode::InvalidArgument, "force must be finite");
    const double scale = *q * stream.referenceArea;
    return ForceCoefficients{force.dot(drag) / scale, force.dot(lift) / scale};
}

Expected<double> pressureCoefficient(double pressure, double freeStreamPressure,
                                     const FreeStream& stream) {
    const auto q = dynamicPressure(stream);
    if (!q)
        return q.error();
    if (!std::isfinite(pressure) || !std::isfinite(freeStreamPressure))
        return Error::make(ErrorCode::InvalidArgument, "pressures must be finite");
    return (pressure - freeStreamPressure) / *q;
}

} // namespace vortia::openfoam
