#pragma once

#include <QCoreApplication>
#include <QFileInfo>
#include <QProcessEnvironment>
#include <QStandardPaths>
#include <QString>
#include <QStringList>

namespace vortia {

/// Directory holding gmsh.py and its native library. Packaged builds carry it
/// next to the executable; VORTIA_GMSH_RUNTIME overrides it for development.
inline QString mesherGmshRuntime() {
    return qEnvironmentVariable("VORTIA_GMSH_RUNTIME",
        QCoreApplication::applicationDirPath() + QStringLiteral("/../Resources/gmsh"));
}

/// Python used to drive Gmsh: the interpreter packaged with Vortia when there
/// is one, otherwise a system Python 3. Empty when none exists.
inline QString mesherPython() {
    QStringList candidates;
    if (qEnvironmentVariableIsSet("VORTIA_PYTHON"))
        candidates << qEnvironmentVariable("VORTIA_PYTHON");
    candidates << QCoreApplication::applicationDirPath() + QStringLiteral("/../Resources/python/bin/python3")
               << QStringLiteral("/opt/homebrew/bin/python3") << QStringLiteral("/usr/local/bin/python3")
               << QStandardPaths::findExecutable(QStringLiteral("python3"));
    for (const QString& candidate : candidates)
        if (!candidate.isEmpty() && QFileInfo(candidate).isExecutable())
            return QFileInfo(candidate).absoluteFilePath();
    return {};
}

/// The packaged interpreter must never write bytecode into the (signed,
/// possibly read-only) application, nor pick up the user's Python settings.
inline void applyMesherEnvironment(QProcessEnvironment& environment) {
    environment.insert(QStringLiteral("PYTHONDONTWRITEBYTECODE"), QStringLiteral("1"));
    environment.remove(QStringLiteral("PYTHONHOME"));
    environment.remove(QStringLiteral("PYTHONPATH"));
}

} // namespace vortia
