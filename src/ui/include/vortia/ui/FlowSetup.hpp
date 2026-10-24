#pragma once
#include "vortia/geometry/Sketch2D.hpp"
#include <QJsonObject>
#include <QString>
#include <string>
#include <vector>
class QWidget;
namespace vortia {
QJsonObject editFlowSetup(QWidget*, const Sketch2D&, const QJsonObject&, int page = 0);
QString generateFlowCase(QWidget*, const QString& sketchPath, const QJsonObject&,
                         const QString& projectRoot,
                         const std::vector<std::pair<std::string,std::string>>& environment);
}
