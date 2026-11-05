#pragma once

/// Roache Grid Convergence Index (GCI) for three successively refined meshes.
/// Reference: P.J. Roache, "Perspective: A Method for Uniform Reporting of Grid
/// Convergence", ASME J. Fluids Eng. 116 (1994); Celik et al., JFE 130 (2008).
///
/// Convention: index 1 = finest, 2 = mid, 3 = coarsest.
/// Refinement ratios r21 = h2/h1 and r32 = h3/h2 must be > 1.

#include <cmath>
#include <string>

namespace vortia::verification {

struct GciInput {
    double f1{0.0}; ///< Quantity of interest on the finest mesh
    double f2{0.0}; ///< Mid mesh
    double f3{0.0}; ///< Coarsest mesh
    double r21{2.0}; ///< h2 / h1 (> 1)
    double r32{2.0}; ///< h3 / h2 (> 1)
    double safetyFactor{1.25}; ///< 1.25 recommended for three-grid studies
};

struct GciResult {
    bool ok{false};
    double observedOrder{0.0};   ///< p_obs
    double extrapolated{0.0};    ///< Richardson extrapolated continuum estimate
    double gci12{0.0};           ///< Fine-grid GCI (relative to |f1| if f1≠0, else absolute)
    double gci23{0.0};           ///< Mid-grid GCI
    double asymptoticRatio{0.0}; ///< gci23 / (r21^p * gci12); ≈1 in asymptotic range
    std::string message;
};

[[nodiscard]] inline bool isFinitePositive(double x) {
    return std::isfinite(x) && x > 0.0;
}

/// Three-grid GCI. Returns ok=false with a message when inputs are invalid or
/// convergence is oscillatory / non-monotonic in a way that blocks a real order.
[[nodiscard]] inline GciResult computeGci(const GciInput& in) {
    GciResult out;
    if (!std::isfinite(in.f1) || !std::isfinite(in.f2) || !std::isfinite(in.f3)) {
        out.message = "solution values must be finite";
        return out;
    }
    if (!isFinitePositive(in.r21) || !isFinitePositive(in.r32) || in.r21 <= 1.0 || in.r32 <= 1.0) {
        out.message = "refinement ratios r21 and r32 must be finite and > 1";
        return out;
    }
    if (!isFinitePositive(in.safetyFactor)) {
        out.message = "safety factor must be finite and > 0";
        return out;
    }

    const double eps21 = in.f2 - in.f1;
    const double eps32 = in.f3 - in.f2;
    if (eps21 == 0.0 && eps32 == 0.0) {
        out.ok = true;
        out.observedOrder = 0.0;
        out.extrapolated = in.f1;
        out.gci12 = 0.0;
        out.gci23 = 0.0;
        out.asymptoticRatio = 1.0;
        out.message = "identical solutions on all three meshes";
        return out;
    }
    if (eps21 == 0.0 || eps32 == 0.0) {
        out.message = "zero consecutive difference; cannot estimate observed order";
        return out;
    }
    // Oscillatory (sign change) — still allow |·| based order with a warning flag in message.
    const bool oscillatory = (eps21 * eps32) < 0.0;
    const double ratio = std::abs(eps32 / eps21);
    if (!(ratio > 0.0) || !std::isfinite(ratio)) {
        out.message = "invalid difference ratio for observed order";
        return out;
    }

    // Celik et al. (2008) iterative form for unequal ratios; closed form when r21≈r32.
    double p = std::log(ratio) / std::log(in.r21);
    if (std::abs(in.r21 - in.r32) > 1e-12) {
        // One fixed-point iteration of the Celik q(p) correction.
        const double q = std::log((std::pow(in.r21, p) - 1.0) / (std::pow(in.r32, p) - 1.0));
        p = (std::log(ratio) + q) / std::log(in.r21);
    }
    if (!std::isfinite(p) || p <= 0.0) {
        out.message = "observed order is non-positive or non-finite";
        return out;
    }

    const double rp21 = std::pow(in.r21, p);
    const double rp32 = std::pow(in.r32, p);
    if (!(rp21 > 1.0) || !(rp32 > 1.0)) {
        out.message = "r^p must exceed 1";
        return out;
    }

    const double fExact = (rp21 * in.f1 - in.f2) / (rp21 - 1.0);
    const double absGci12 = in.safetyFactor * std::abs(eps21) / (rp21 - 1.0);
    const double absGci23 = in.safetyFactor * std::abs(eps32) / (rp32 - 1.0);

    // Report relative GCI when |f1| is meaningful; else absolute.
    const double scale1 = std::abs(in.f1) > 0.0 ? std::abs(in.f1) : 1.0;
    const double scale2 = std::abs(in.f2) > 0.0 ? std::abs(in.f2) : 1.0;

    out.ok = true;
    out.observedOrder = p;
    out.extrapolated = fExact;
    out.gci12 = absGci12 / scale1;
    out.gci23 = absGci23 / scale2;
    // Asymptotic check uses absolute GCIs (Celik); relative scales differ by |f|.
    out.asymptoticRatio = (absGci12 > 0.0) ? absGci23 / (rp21 * absGci12) : 0.0;
    out.message = oscillatory ? "oscillatory convergence (sign change in consecutive differences)"
                              : "ok";
    return out;
}

} // namespace vortia::verification
