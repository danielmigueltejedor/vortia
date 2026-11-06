#pragma once

/// Discrete flux conservation checks for incompressible verification cases.
/// Callers must supply **non-negative flow magnitudes** (volume or mass rates
/// as positive scalars). OpenFOAM signed monitors must be converted with
/// `std::abs` before calling. Relative imbalance =
/// |Qin − Qout| / max(Qin, Qout) when that max is > 0.

#include <algorithm>
#include <cmath>
#include <string>

namespace vortia::verification {

struct FluxConservationInput {
    double inletFlux{0.0};  ///< Non-negative inlet flow magnitude
    double outletFlux{0.0}; ///< Non-negative outlet flow magnitude
    double relativeTolerance{1e-3}; ///< a-priori acceptance band
};

struct FluxConservationResult {
    bool ok{false};
    double relativeImbalance{0.0};
    double absoluteImbalance{0.0};
    std::string message;
};

[[nodiscard]] inline FluxConservationResult assessVolumeFluxBalance(const FluxConservationInput& in) {
    FluxConservationResult out;
    if (!std::isfinite(in.inletFlux) || !std::isfinite(in.outletFlux) ||
        !std::isfinite(in.relativeTolerance) || in.relativeTolerance <= 0.0) {
        out.message = "fluxes and tolerance must be finite; tolerance > 0";
        return out;
    }
    if (in.inletFlux < 0.0 || in.outletFlux < 0.0) {
        out.message = "fluxes must be non-negative magnitudes (use abs on signed monitors)";
        return out;
    }
    out.absoluteImbalance = std::abs(in.inletFlux - in.outletFlux);
    const double scale = std::max(in.inletFlux, in.outletFlux);
    if (!(scale > 0.0)) {
        out.relativeImbalance = 0.0;
        out.ok = true;
        out.message = "both fluxes zero";
        return out;
    }
    out.relativeImbalance = out.absoluteImbalance / scale;
    out.ok = out.relativeImbalance <= in.relativeTolerance;
    out.message = out.ok ? "within tolerance" : "imbalance exceeds tolerance";
    return out;
}

} // namespace vortia::verification
