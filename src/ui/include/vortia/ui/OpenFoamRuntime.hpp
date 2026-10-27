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
    return {};
#endif
}
} // namespace vortia
