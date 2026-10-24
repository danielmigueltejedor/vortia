#include "vortia/solver/openfoam/LogParser.hpp"

#include <cmath>
#include <regex>

namespace vortia::openfoam {

namespace {

// smoothSolver:  Solving for Ux, Initial residual = 1, Final residual = 2.96335e-06, No Iterations 19
const std::regex kResidual(
    R"(\bSolving for (\w+), Initial residual = ([0-9.eE+\-nanif]+), Final residual = ([0-9.eE+\-nanif]+), No Iterations (\d+))");

const std::regex kTime(R"(^\s*Time = ([0-9.eE+\-]+)\s*$)");

const std::regex kCourant(R"(Courant Number mean: ([0-9.eE+\-nanif]+) max: ([0-9.eE+\-nanif]+))");
const std::regex kCentralCourant(R"(Courant Numbers: min = [0-9.eE+\-nanif]+, average = ([0-9.eE+\-nanif]+), max = ([0-9.eE+\-nanif]+))");

[[nodiscard]] bool looksFatal(const std::string& line) {
    return line.find("FOAM FATAL") != std::string::npos ||
           line.find("floating point exception", 0) != std::string::npos ||
           line.find("FOAM SIGFPE") != std::string::npos;
}

[[nodiscard]] bool isBadNumber(double v) { return std::isnan(v) || std::isinf(v); }

} // namespace

void LogParser::feed(std::string_view chunk) {
    pending_.append(chunk);
    std::size_t pos = 0;
    while ((pos = pending_.find('\n')) != std::string::npos) {
        std::string line = pending_.substr(0, pos);
        if (!line.empty() && line.back() == '\r')
            line.pop_back();
        pending_.erase(0, pos + 1);
        processLine(line);
    }
}

void LogParser::flush() {
    if (!pending_.empty()) {
        std::string line = pending_;
        if (!line.empty() && line.back() == '\r')
            line.pop_back();
        pending_.clear();
        processLine(line);
    }
}

void LogParser::reset() {
    pending_.clear();
    progress_ = SolverProgress{};
}

void LogParser::processLine(const std::string& line) {
    if (looksFatal(line)) {
        progress_.diverged = true;
        progress_.divergenceReason = line;
        return;
    }

    std::smatch match;
    if (std::regex_search(line, match, kTime)) {
        try {
            progress_.time = std::stod(match[1].str());
        } catch (...) {
        }
        return;
    }
    if (std::regex_search(line, match, kCourant) || std::regex_search(line, match, kCentralCourant)) {
        try {
            progress_.courantMean = std::stod(match[1].str());
            progress_.courantMax = std::stod(match[2].str());
        } catch (...) {
        }
        if (isBadNumber(progress_.courantMax)) {
            progress_.diverged = true;
            progress_.divergenceReason = "Courant number became NaN/Inf";
        }
        return;
    }
    if (std::regex_search(line, match, kResidual)) {
        ResidualSample sample;
        sample.field = match[1].str();
        try {
            sample.initial = std::stod(match[2].str());
            sample.final = std::stod(match[3].str());
            sample.iterations = std::stoi(match[4].str());
        } catch (...) {
            return;
        }
        if (isBadNumber(sample.initial) || isBadNumber(sample.final)) {
            progress_.diverged = true;
            progress_.divergenceReason = "NaN/Inf residual for field " + sample.field;
        }
        for (auto& existing : progress_.residuals) {
            if (existing.field == sample.field) {
                existing = sample;
                return;
            }
        }
        progress_.residuals.push_back(std::move(sample));
    }
}

} // namespace vortia::openfoam
