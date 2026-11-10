#pragma once

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QStandardPaths>
#include <QStringList>
#include <filesystem>

namespace vortia {
inline std::filesystem::path bundledOpenFoamRoot() {
#ifdef Q_OS_MACOS
    return QDir(QCoreApplication::applicationDirPath())
        .absoluteFilePath("../Resources/OpenFOAM-v2606").toStdString();
#else
    // Linux AppImage: <AppDir>/usr/lib/openfoam-engine/OpenFOAM-<version>.
    const QDir engine(QCoreApplication::applicationDirPath() + QStringLiteral("/../lib/openfoam-engine"));
    const QStringList roots = engine.entryList({QStringLiteral("OpenFOAM-*")}, QDir::Dirs | QDir::NoDotAndDotDot,
                                               QDir::Name);
    if (roots.isEmpty())
        return {};
    const QString enginePath = QDir::cleanPath(engine.absolutePath());
    const auto hasBlank = [](const QString& path) {
        for (const QChar c : path)
            if (c.isSpace())
                return true;
        return false;
    };
    if (!hasBlank(enginePath))
        return (enginePath + QLatin1Char('/') + roots.last()).toStdString();
    // OpenFOAM's own scripts break when its path contains blanks (an AppImage
    // unpacked under "~/Mis aplicaciones", for instance). Reach the engine
    // through a blank-free symbolic link instead.
    const QString tag = QString::fromLatin1(
        QCryptographicHash::hash(enginePath.toUtf8(), QCryptographicHash::Sha256).toHex().left(16));
    for (const QString& base : {QStandardPaths::writableLocation(QStandardPaths::CacheLocation),
                                QDir::tempPath()}) {
        if (base.isEmpty() || hasBlank(base) || !QDir().mkpath(base))
            continue;
        const QString link = base + QStringLiteral("/vortia-engine-") + tag;
        const QFileInfo existing(link);
        if (existing.isSymLink() && existing.symLinkTarget() != enginePath)
            QFile::remove(link);
        if (!QFileInfo(link).isSymLink() && !QFile::link(enginePath, link))
            continue;
        if (QFileInfo(link + QLatin1Char('/') + roots.last() + QStringLiteral("/etc/bashrc")).isFile())
            return (link + QLatin1Char('/') + roots.last()).toStdString();
    }
    return (enginePath + QLatin1Char('/') + roots.last()).toStdString();
#endif
}
} // namespace vortia
