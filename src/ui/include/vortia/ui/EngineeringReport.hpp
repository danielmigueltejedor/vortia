#pragma once
#include "vortia/visualization/SectionAnalysis.hpp"
#include <QJsonObject>
#include <QJsonArray>
#include <QStringList>
namespace vortia {
QJsonObject makeEngineeringReport(const SectionOptions&,const SectionAnalysis&,const QJsonObject& source,const QString& label);
bool validEngineeringReport(const QJsonObject&);
QString engineeringReportHtml(const QJsonObject&);
QByteArray engineeringReportCsv(const QJsonObject&);
QByteArray engineeringComparisonCsv(const QJsonArray&,const QString& metric,int baseline);
QStringList engineeringMetricKeys();
QString engineeringMetricTitle(const QString&);
QString engineeringComparisonWarning(const QJsonObject&,const QJsonObject&);
}
