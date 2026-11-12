#pragma once
#include "vortia/geometry/Sketch2D.hpp"
#include <QJsonObject>
namespace vortia {
/// The compressible-flow settings of a sketch, as the flow setup panel shows them before anyone
/// edits a field: every value from `previous`, else `defaults`, else the built-in default,
/// clamped and rounded exactly as the panel's controls do. QtCore only, so vortia-engine shares it.
QJsonObject flowConfiguration(const Sketch2D& shape, const QJsonObject& previous = {},
                              const QJsonObject& defaults = {});
}
