#pragma once

/// Roache Grid Convergence Index (GCI) for three successively refined meshes.
/// Reference: P.J. Roache, ASME J. Fluids Eng. 116 (1994); Celik et al., JFE 130 (2008).
///
/// Convention: index 1 = finest, 2 = mid, 3 = coarsest.
/// Refinement ratios r21 = h2/h1 and r32 = h3/h2 must be > 1.
/// Oscillatory triples and equal-ratio divergent sequences (|ε32/ε21| ≤ 1) are rejected.

#include <algorithm>
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
    /// Fine-grid GCI. Relative to |f1| when relativeGci is true; otherwise absolute.
    double gci12{0.0};
    /// Mid-grid GCI. Relative to |f2| when relativeGci is true; otherwise absolute.
    /// relativeGci is true only when both |f1| and |f2| are nonzero.
    double gci23{0.0};
    bool relativeGci{true}; ///< false ⇒ gci12/gci23 are absolute (a QoI scale was zero)
    double asymptoticRatio{0.0}; ///< gci23 / (r21^p · gci12) using the reported GCIs
    std::string message;
};

[[nodiscard]] inline bool isFinitePositive(double x) {
    return std::isfinite(x) && x > 0.0;
}

/// True for ±0 and for bit-identical values (0.0 and -0.0 count as equal).
[[nodiscard]] inline bool numericallyEqual(double a, double b) {
    return std::fabs(a - b) <= 0.0;
}

[[nodiscard]] inline bool isNumericZero(double x) {
    return std::fabs(x) <= 0.0;
}

/// Three-grid GCI. Returns ok=false when inputs are invalid, oscillatory,
/// equal-ratio divergent, or the observed-order iteration does not converge.
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

    if (numericallyEqual(in.f1, in.f2) && numericallyEqual(in.f2, in.f3)) {
        out.ok = true;
        out.observedOrder = 0.0;
        out.extrapolated = in.f1 + 0.0; // canonical +0
        out.gci12 = 0.0;
        out.gci23 = 0.0;
        out.relativeGci = true;
        out.asymptoticRatio = 1.0;
        out.message = "identical solutions on all three meshes";
        return out;
    }
    if (isNumericZero(eps21) || isNumericZero(eps32)) {
        out.message = "zero consecutive difference; cannot estimate observed order";
        return out;
    }
    // Compare signs directly — products of tiny values can underflow to ±0.
    if (std::signbit(eps21) != std::signbit(eps32)) {
        out.message = "oscillatory convergence; monotonic GCI does not apply";
        return out;
    }

    const double absRatio = std::abs(eps32 / eps21);
    if (!(absRatio > 0.0) || !std::isfinite(absRatio)) {
        out.message = "invalid difference ratio for observed order";
        return out;
    }

    const bool equalRatios = std::abs(in.r21 - in.r32) <= 1e-12;
    // Equal-r studies require |ε32/ε21| > 1 (errors shrink toward the fine mesh).
    // Taking abs(log) here would invent a positive order for divergent data.
    if (equalRatios && !(absRatio > 1.0)) {
        out.message = "equal-ratio study requires |eps32/eps21| > 1 (positive observed order)";
        return out;
    }

    // Celik et al. (2008): p = (1/ln r21) |ln|ε32/ε21| + q(p)| with
    // q(p) = ln((r21^p − s)/(r32^p − s)), s = +1 for monotonic triples.
    constexpr double s = 1.0;
    double p = equalRatios ? (std::log(absRatio) / std::log(in.r21))
                           : (std::abs(std::log(absRatio)) / std::log(in.r21));
    if (!equalRatios) {
        bool converged = false;
        for (int iter = 0; iter < 50; ++iter) {
            const double rp21 = std::pow(in.r21, p);
            const double rp32 = std::pow(in.r32, p);
            const double num = rp21 - s;
            const double den = rp32 - s;
            if (!(num > 0.0) || !(den > 0.0) || !std::isfinite(num) || !std::isfinite(den)) {
                out.message = "observed-order iteration left the valid domain";
                return out;
            }
            const double q = std::log(num / den);
            const double pNew = std::abs(std::log(absRatio) + q) / std::log(in.r21);
            if (!std::isfinite(pNew) || pNew <= 0.0) {
                out.message = "observed order is non-positive or non-finite";
                return out;
            }
            if (std::abs(pNew - p) < 1e-10 * std::max(1.0, std::abs(pNew))) {
                p = pNew;
                converged = true;
                break;
            }
            p = pNew;
        }
        if (!converged) {
            out.message = "observed-order iteration did not converge";
            return out;
        }
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

    const double scale1 = std::abs(in.f1);
    const double scale2 = std::abs(in.f2);
    const bool relative = scale1 > 0.0 && scale2 > 0.0;
    out.relativeGci = relative;
    out.gci12 = relative ? absGci12 / scale1 : absGci12;
    out.gci23 = relative ? absGci23 / scale2 : absGci23;

    if (!(out.gci12 > 0.0)) {
        out.message = "fine-grid GCI is zero; asymptotic ratio undefined";
        return out;
    }
    out.asymptoticRatio = out.gci23 / (rp21 * out.gci12);

    out.ok = true;
    out.observedOrder = p;
    out.extrapolated = fExact;
    out.message = relative ? "ok" : "ok (absolute GCI; QoI scale was zero)";
    return out;
}

} // namespace vortia::verification
