#pragma once

#include <cmath>
#include <cstddef>
#include <vector>

namespace vortia::verification {

struct ProfileError {
    double l2{0.0};       ///< RMS of absolute error over samples
    double linf{0.0};     ///< max |error|
    double relL2{0.0};    ///< L2(error) / L2(reference); 0 if reference norm is 0
    double relLinf{0.0};  ///< Linf(error) / max(|ref|); 0 if max|ref| is 0
    std::size_t count{0};
};

/// Pairwise absolute errors between reference and candidate samples of equal length.
[[nodiscard]] inline ProfileError compareProfiles(const std::vector<double>& reference,
                                                  const std::vector<double>& candidate) {
    ProfileError out;
    if (reference.size() != candidate.size() || reference.empty())
        return out;
    out.count = reference.size();
    double sumSq = 0.0;
    double sumSqRef = 0.0;
    double maxAbsRef = 0.0;
    for (std::size_t i = 0; i < reference.size(); ++i) {
        const double e = candidate[i] - reference[i];
        const double ae = std::abs(e);
        sumSq += e * e;
        sumSqRef += reference[i] * reference[i];
        maxAbsRef = std::max(maxAbsRef, std::abs(reference[i]));
        out.linf = std::max(out.linf, ae);
    }
    out.l2 = std::sqrt(sumSq / static_cast<double>(out.count));
    const double refNorm = std::sqrt(sumSqRef / static_cast<double>(out.count));
    if (refNorm > 0.0)
        out.relL2 = out.l2 / refNorm;
    if (maxAbsRef > 0.0)
        out.relLinf = out.linf / maxAbsRef;
    return out;
}

} // namespace vortia::verification
