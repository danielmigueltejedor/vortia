#pragma once

#include <string>
#include <string_view>
#include <vector>

namespace vortia::openfoam {

struct ResidualSample {
    std::string field;
    double initial{0.0};
    double final{0.0};
    int iterations{0};
};

/// Incrementally-built view of a running solver's numerical state.
struct SolverProgress {
    double time{0.0};
    double courantMean{0.0};
    double courantMax{0.0};
    std::vector<ResidualSample> residuals; ///< Latest sample per field.
    bool diverged{false};
    std::string divergenceReason;
};

/// Incremental parser for OpenFOAM solver logs (icoFoam/simpleFoam style).
/// Feed output chunks as they arrive; complete lines are matched against the
/// documented log formats:
///   Time = 0.005
///   Courant Number mean: 0.029 max: 0.585
///   smoothSolver:  Solving for Ux, Initial residual = 1, Final residual = 2.9e-06, No Iterations 19
///   DICPCG:  Solving for p, Initial residual = ...
/// Divergence is detected on NaN/Inf residuals and FOAM fatal signatures.
class LogParser {
public:
    void feed(std::string_view chunk);
    [[nodiscard]] const SolverProgress& progress() const { return progress_; }
    void reset();

private:
    void processLine(const std::string& line);

    std::string pending_; ///< Partial line not yet terminated.
    SolverProgress progress_;
};

} // namespace vortia::openfoam
