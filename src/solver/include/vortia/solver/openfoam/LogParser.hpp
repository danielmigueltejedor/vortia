#pragma once

#include <string>
#include <string_view>
#include <vector>
#include <optional>

namespace vortia::openfoam {

struct ResidualSample {
    std::string field;
    double initial{0.0};
    double final{0.0};
    int iterations{0};
};

struct ConvergenceSample {
    std::string status;
    double iteration=0, checkedIteration=0, globalMass=0, localMass=0, fieldChange=0;
    double massTolerance=0, fieldTolerance=0, equationError=0, equationTolerance=0;
    int stableSamples=0, requiredSamples=0, interval=0;
    bool sampled=false, automatic=false;
    std::optional<double> massIncoming,massOutgoing;
};
// Strict parsing shared by live progress and persisted assessment readers.
std::optional<ConvergenceSample> parseConvergenceReport(std::string_view json);

enum class DivergenceKind { None, Fatal, NonFinite, NonPositiveUpdate };

/// Incrementally-built view of a running solver's numerical state.
struct SolverProgress {
    std::optional<ConvergenceSample> convergence;
    double time{0.0};
    double courantMean{0.0};
    double courantMax{0.0};
    std::vector<ResidualSample> residuals; ///< Latest sample per field.
    bool diverged{false};
    DivergenceKind divergenceKind{DivergenceKind::None};
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
struct MonitorSample {
    double coordinate=0,value=0;
    std::string series;
    int session=0;
};
class LogParser {
public:
    explicit LogParser(bool collectMonitors=false):collectMonitors_(collectMonitors){}
    std::vector<MonitorSample> takeMonitorSamples();
    void feed(std::string_view chunk);
    /// Processes any trailing partial line (call when the stream ends).
    void flush();
    [[nodiscard]] const SolverProgress& progress() const { return progress_; }
    void reset();

private:
    void processLine(const std::string& line);

    std::string pending_; ///< Partial line not yet terminated.
    SolverProgress progress_;
    void monitor(const std::string&,double,double);
    bool collectMonitors_=false,hasTime_=false,historyPhase_=true;
    int session_=0;
    std::vector<MonitorSample> monitorSamples_;
    std::optional<std::pair<double,double>> pendingCentralCourant_;
    std::optional<std::pair<double,double>> clockBaseline_;

};

} // namespace vortia::openfoam
