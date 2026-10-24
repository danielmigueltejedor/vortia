#pragma once
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonArray>
#include <QJsonObject>
#include <cmath>
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
}
