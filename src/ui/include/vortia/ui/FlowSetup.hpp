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
// Update only runtime controls in a staged dictionary, preserving all saved fields.
// Returns an error message, or an empty string on success.
QString updateFlowRunControls(const QString& caseDir, const QJsonObject& solver,
                             const std::vector<std::pair<std::string,std::string>>& environment);
// Stage a content-addressed bundled backend without replacing legacy sources,
// executables, boundary dictionaries, mesh or saved fields. Empty = success.
QString stageFlowBackend(const QString& caseDir);
}
