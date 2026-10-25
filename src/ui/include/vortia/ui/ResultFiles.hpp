#pragma once
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonArray>
#include <QJsonObject>
#include <cmath>
#include <algorithm>
#include <QDateTime>
#include <limits>

namespace vortia {
/// foamToVTK names files by its time INDEX, not necessarily the solver time.
/// Prefer the series metadata; never select nested boundary-only datasets.
inline QString latestVtmFile(const QString& directory) {
    QDir dir(directory);
    QString latest;
    double best=-std::numeric_limits<double>::infinity();
    for (const auto& name : dir.entryList({QStringLiteral("*.vtm.series")},QDir::Files)) {
        QFile file(dir.filePath(name));
        if (!file.open(QIODevice::ReadOnly) || file.size()>8*1024*1024) continue;
        const auto items=QJsonDocument::fromJson(file.readAll()).object().value("files").toArray();
        for (const auto& item : items) {
            const auto obj=item.toObject();
            const QString entry=obj.value("name").toString();
            const double time=obj.value("time").toDouble(std::numeric_limits<double>::quiet_NaN());
            if (QFileInfo(entry).fileName()!=entry || !entry.endsWith(QStringLiteral(".vtm"))
                || !QFileInfo::exists(dir.filePath(entry)) || !std::isfinite(time)) continue;
            if (time>best) { best=time; latest=dir.filePath(entry); }
        }
    }
    if (!latest.isEmpty()) return latest;
    for (const auto& name : dir.entryList({QStringLiteral("*.vtm")},QDir::Files)) {
        bool ok=false;
        const double index=QFileInfo(name).completeBaseName().section('_',-1).toDouble(&ok);
        if (ok && std::isfinite(index) && index>best) {best=index;latest=dir.filePath(name);}
    }
    return latest;
}
// Avoid silently displaying an old export after a native-only continuation.
inline QString currentVtmFile(const QString& caseDirectory) {
    if(caseDirectory.isEmpty())return {};
    const auto selected=latestVtmFile(QDir(caseDirectory).filePath("VTK"));
    if(selected.isEmpty())return {};
    double native=-std::numeric_limits<double>::infinity();QFileInfo checkpoint;
    QDir root(caseDirectory);
    for(const auto& entry:root.entryList(QDir::Dirs|QDir::NoDotAndDotDot)) {
        bool ok=false;const double t=entry.toDouble(&ok);QFileInfo field(root.filePath(entry+"/p"));
        if(ok&&std::isfinite(t)&&field.isFile()&&t>native){native=t;checkpoint=field;}
    }
    if(!std::isfinite(native))return selected;
    QDir vtk(root.filePath("VTK"));
    for(const auto& name:vtk.entryList({"*.vtm.series"},QDir::Files)) {
        QFile series(vtk.filePath(name));if(!series.open(QIODevice::ReadOnly)||series.size()>8*1024*1024)continue;
        for(const auto& item:QJsonDocument::fromJson(series.readAll()).object().value("files").toArray()) {
            const auto obj=item.toObject();if(obj.value("name").toString()!=QFileInfo(selected).fileName())continue;
            const double time=obj.value("time").toDouble(std::numeric_limits<double>::quiet_NaN());
            return std::isfinite(time)&&std::abs(time-native)<=1e-9*std::max(1.0,std::abs(native))?selected:QString{};
        }
    }
    return QFileInfo(selected).lastModified()>=checkpoint.lastModified()?selected:QString{};
}

}
