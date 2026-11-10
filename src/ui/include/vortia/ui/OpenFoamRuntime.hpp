#pragma once

#include <QCoreApplication>
#include <QDir>
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
    return QDir::cleanPath(engine.absoluteFilePath(roots.last())).toStdString();
#endif
}
} // namespace vortia
