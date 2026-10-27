#pragma once
#include "vortia/solver/openfoam/LogParser.hpp"
#include <QJsonObject>
#include <QString>
#include <optional>

namespace vortia {
QJsonObject diagnoseRunFailure(openfoam::DivergenceKind kind,const QString& summary,
    const QString& evidence,const QString& phase,double iteration);
bool saveRunDiagnostic(const QString& caseDir,const QJsonObject& report);
QJsonObject loadRunDiagnostic(const QString& caseDir);
bool offersFreshFlowStart(const QJsonObject& diagnostic);
QString runDiagnosticHtml(const QJsonObject& diagnostic,double courant);
std::optional<QJsonObject> conservativeRestartConfiguration(const QJsonObject& configuration);
} // namespace vortia
