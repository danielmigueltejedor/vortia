#pragma once
#include "vortia/geometry/Sketch2D.hpp"
#include <QJsonObject>
#include <QJsonArray>
namespace vortia {
QJsonObject boundaryGeometry(const Sketch2D&);
QJsonObject reconcileFlowBoundaries(const Sketch2D& before,const Sketch2D& after,const QJsonObject& config,bool remember=true);
QJsonObject restoreBoundaryGeometry(const Sketch2D& current,const QJsonObject& config);
QJsonObject normalizedFlowConfiguration(const Sketch2D&,const QJsonObject&);
QJsonArray normalizedBoundaryNames(const QJsonArray& boundaries);
}
