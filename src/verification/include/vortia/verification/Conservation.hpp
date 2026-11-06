#pragma once

/// Discrete flux conservation checks for incompressible verification cases.
/// These compare measured inlet/outlet volume (or mass) fluxes; they do not
/// invent CFD fields. Relative imbalance = |Qin − Qout| / max(|Qin|, |Qout|, ε).

#include <algorithm>
#include <cmath>
#include <string>

namespace vortia::verification {

struct FluxConservationInput {
    double inletFlux{0.0};
    double outletFlux{0.0};
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
    out.absoluteImbalance = std::abs(in.inletFlux - in.outletFlux);
    const double scale = std::max(std::abs(in.inletFlux), std::abs(in.outletFlux));
    if (!(scale > 0.0)) {
        // Both fluxes are exactly zero → perfectly balanced empty system.
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
