#pragma once
#include "vortia/geometry/Sketch2D.hpp"
#include <QJsonObject>
#include <QString>
#include <string>
#include <vector>
#include <cstdint>
#include <algorithm>
class QWidget;
namespace vortia {
// Keep small meshes out of MPI launch/communication overhead. Explicit user
// process counts always bypass this conservative desktop recommendation.
inline unsigned recommendedFlowProcesses(std::uint64_t cells,unsigned hardwareThreads) {
    const unsigned budget=std::clamp(hardwareThreads,1u,8u);
    const auto useful=std::clamp<std::uint64_t>(cells/3000,1,budget);
    if(useful==budget)return budget;
    unsigned chosen=1;while(chosen*2<=useful)chosen*=2;return chosen;
}
QJsonObject editFlowSetup(QWidget*, const Sketch2D&, const QJsonObject&, int page = 0, const QJsonObject& defaults = {});
QString generateFlowCase(QWidget*, const QString& sketchPath, const QJsonObject&,
                         const QString& projectRoot,
                         const std::vector<std::pair<std::string,std::string>>& environment, const QString& caseStorage = {});
// Update only runtime controls in a staged dictionary, preserving all saved fields.
// Returns an error message, or an empty string on success.
QString updateFlowRunControls(const QString& caseDir, const QJsonObject& solver,
                             const std::vector<std::pair<std::string,std::string>>& environment);
// Stage a content-addressed bundled backend without replacing legacy sources,
// executables, boundary dictionaries, mesh or saved fields. Empty = success.
QString stageFlowBackend(const QString& caseDir);
}
