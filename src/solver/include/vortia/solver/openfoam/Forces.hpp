#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/core/Vec3.hpp"

#include <string>
#include <string_view>
#include <vector>

namespace vortia::openfoam {

// ---------------------------------------------------------------------------
// Wall forces: OpenFOAM "forces" function object
// ---------------------------------------------------------------------------

struct ForcesRequest {
    std::vector<std::string> patches; ///< Wall patches to integrate over.
    /// Reference density [kg/m³]. Incompressible solvers work with kinematic
    /// pressure, so OpenFOAM multiplies by this value; 1 yields force per
    /// unit density [m⁴/s²].
    double rhoInf{1.0};
    Vec3 centreOfRotation{}; ///< Moment reference point [m].
};

/// Body of a `system/forces` dictionary for `<solver> -postProcess -func forces`.
/// Patch names are validated (OpenFOAM words only) so they cannot inject
/// dictionary syntax.
[[nodiscard]] Expected<std::string> forcesFunctionObject(const ForcesRequest& request);

/// One row of a forces file. Forces are those exerted BY the fluid ON the
/// patches, in newtons when rhoInf is the real density.
struct ForceSample {
    double time{0.0};
    Vec3 total{};
    Vec3 pressure{};
    Vec3 viscous{};
};

/// Parses `postProcessing/forces/<time>/force.dat` (ESI v2012 and newer:
/// total/pressure/viscous columns) and the older bracketed `forces.dat`
/// (`time ((pressure) (viscous) ...) ...`, also written by Foundation
/// releases). Returns rows in file order; an input without data rows is an
/// error, never an empty success.
[[nodiscard]] Expected<std::vector<ForceSample>> parseForces(std::string_view text);

// ---------------------------------------------------------------------------
// Aerodynamic coefficients
// ---------------------------------------------------------------------------

struct FreeStream {
    double density{1.0};       ///< ρ∞ [kg/m³]; use 1 with per-unit-density forces.
    double speed{0.0};         ///< U∞ [m/s]
    double referenceArea{0.0}; ///< A_ref [m²]
};

/// q∞ = ½ ρ∞ U∞² [Pa, or m²/s² for ρ∞ = 1].
[[nodiscard]] Expected<double> dynamicPressure(const FreeStream& stream);

struct ForceCoefficients {
    double drag{0.0}; ///< C_D = F·d̂ / (q∞ A_ref)
    double lift{0.0}; ///< C_L = F·l̂ / (q∞ A_ref)
};

/// Drag and lift coefficients of `force` along the given directions. The
/// directions need not be unit vectors but must be non-zero and mutually
/// perpendicular (|d̂·l̂| ≤ 1e-9).
[[nodiscard]] Expected<ForceCoefficients>
forceCoefficients(const Vec3& force, const Vec3& dragDirection, const Vec3& liftDirection,
                  const FreeStream& stream);

/// C_p = (p − p∞) / q∞. Pressures and q∞ must use the same convention
/// (both static, or both kinematic with ρ∞ = 1).
[[nodiscard]] Expected<double> pressureCoefficient(double pressure, double freeStreamPressure,
                                                   const FreeStream& stream);

} // namespace vortia::openfoam
