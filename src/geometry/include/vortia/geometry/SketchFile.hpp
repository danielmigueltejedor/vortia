#pragma once

#include "vortia/core/Expected.hpp"
#include "vortia/geometry/Sketch2D.hpp"
#include "vortia/geometry/SketchTemplates.hpp"

#include <string>
#include <string_view>

namespace vortia {

/// A `.vsketch.json` file: the fluid contour in millimetres, its arcs as segment bulges, and the
/// nozzle dimensions it was made from. Shared by the desktop editor and vortia-engine.
struct SketchDocument {
    Sketch2D sketch;
    NozzleDimensions nozzle;
};

/// Schema 1 for straight segments only, 2 once any segment is an arc.
[[nodiscard]] std::string sketchToJson(const SketchDocument& document);

/// Strict: a file that is not exactly a sketch Vortia wrote is refused, never half-loaded.
[[nodiscard]] Expected<SketchDocument> sketchFromJson(std::string_view text);

} // namespace vortia
