#include "vortia/ui/FlowCase.hpp"

#include "vortia/ui/MesherRuntime.hpp"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QProcess>
#include <QProcessEnvironment>
#include <QStringList>
#include <QTemporaryDir>
#include <QUuid>

#include <algorithm>

namespace vortia {
namespace {
QString caseTr(const char* s) { return QCoreApplication::translate("vortia::FlowSetup", s); }
bool hasBlank(const QString& path) {
    return std::any_of(path.cbegin(), path.cend(), [](QChar c) { return c.isSpace(); });
}
} // namespace

QString flowCaseStorage(const QString& projectRoot, const QString& caseStorage, QString* error) {
    QString parentDir = QDir(projectRoot).filePath("cases");
#ifdef Q_OS_MACOS
    // Keep solver writes/builds off iCloud-managed Documents/Desktop folders.
    // The small editable project retains the absolute reference to its results.
    parentDir = QDir::homePath() + "/Library/Vortia/cases";
#endif
    if (!caseStorage.isEmpty()) {
        if (!QDir::isAbsolutePath(caseStorage) || hasBlank(caseStorage)) {
            if (error) *error = caseTr("Choose an absolute calculation path without spaces in Preferences.");
            return {};
        }
        parentDir = caseStorage;
    }
    if (hasBlank(parentDir))
        parentDir = QDir::homePath() + "/Library/Vortia/cases";
    if (!QDir().mkpath(parentDir)) {
        if (error) *error = caseTr("The calculation storage folder could not be created. Check its location and permissions.");
        return {};
    }
    return parentDir;
}

FlowCaseResult prepareFlowCase(const QString& sketchPath, const QJsonObject& cfg, const QString& projectRoot,
                               const std::vector<std::pair<std::string, std::string>>& environment,
                               const QString& caseStorage, const std::function<bool()>& cancelled) {
    FlowCaseResult result;
    const QString parentDir = flowCaseStorage(projectRoot, caseStorage, &result.error);
    if (parentDir.isEmpty()) return result;
    QTemporaryDir stage(parentDir + "/.build-XXXXXX");
    if (!stage.isValid()) {
        result.error = caseTr("The calculation storage folder could not be created. Check its location and permissions.");
        return result;
    }
    const auto write = [&](const QString& name, const QByteArray& bytes) {
        QFile f(stage.filePath(name));
        return f.open(QIODevice::WriteOnly) && f.write(bytes) == bytes.size();
    };
    const auto copyResource = [&](const QString& resource, const QString& name) {
        QFile f(resource);
        return f.open(QIODevice::ReadOnly) && write(name, f.readAll());
    };
    for (const auto& name : {"prepare.py", "boundaries.py", "mesh_regions.py"})
        if (!copyResource(QString(":/vortia/flow/") + name, name)) {
            result.error = caseTr("The bundled mesher is incomplete.");
            return result;
        }
    auto settings = cfg;
    settings.remove("activeCase");
    settings.remove("preparedHash");
    settings.remove("preparedEngine");
    if (!write("settings.json", QJsonDocument(settings).toJson()) || !QDir().mkpath(stage.filePath("solver/Make"))) {
        result.error = caseTr("The calculation storage folder could not be created. Check its location and permissions.");
        return result;
    }
    for (const auto& name : QStringList{"vortiaCentralFoam.C", "ConservativeUpdate.H", "ConvergencePolicy.H",
                                        "ConvergenceMonitor.H", "centralCourantNo.H", "createFieldRefs.H",
                                        "createFields.H", "directionInterpolate.H", "readFluxScheme.H",
                                        "setRDeltaT.H", "fixedRhoFvPatchScalarField.H", "Make/files",
                                        "Make/options", "README.md", "LICENSE.md"})
        if (!copyResource(":/vortia/openfoam/solver/" + name, "solver/" + name)) {
            result.error = caseTr("The bundled flow backend is incomplete.");
            return result;
        }
    const QString python = mesherPython();
    const QString runtime = mesherGmshRuntime();
    if (python.isEmpty() || !QFileInfo::exists(runtime + "/gmsh.py")) {
        result.error = caseTr("This installation is missing Python 3 or the bundled Gmsh runtime.");
        return result;
    }
    QProcess process;
    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    for (const auto& [key, value] : environment)
        env.insert(QString::fromStdString(key), QString::fromStdString(value));
    applyMesherEnvironment(env);
    process.setProcessEnvironment(env);
    process.setProcessChannelMode(QProcess::MergedChannels);
    process.start(python, {stage.filePath("prepare.py"), "--sketch", sketchPath, "--config", stage.filePath("settings.json"),
                           "--case", stage.filePath("case"), "--solver-sources", stage.filePath("solver"),
                           "--gmsh-runtime", runtime});
    bool finished = process.waitForStarted(10000);
    QByteArray output;
    while (finished && process.state() != QProcess::NotRunning) {
        if (cancelled && cancelled()) {
            process.kill();
            process.waitForFinished(3000);
            result.cancelled = true;
            result.error = caseTr("Mesh generation was cancelled.");
            return result;
        }
        process.waitForFinished(100);
        output += process.readAll();
    }
    output += process.readAll();
    finished = finished && process.exitStatus() == QProcess::NormalExit && process.exitCode() == 0;
    if (!finished) {
        result.diagnostic = output;
        QFile converter(stage.filePath("case/log.gmshToFoam"));
        if (converter.open(QIODevice::ReadOnly)) result.diagnostic += "\n" + converter.readAll();
        QDir().mkpath(QDir(projectRoot).filePath("logs"));
        QFile saved(QDir(projectRoot).filePath("logs/mesh-generation.log"));
        if (saved.open(QIODevice::WriteOnly)) saved.write(result.diagnostic);
        result.error = caseTr("Check the contour, boundary assignments and mesh sizes. Details are saved in the project's logs folder.");
        return result;
    }
    QFile log(stage.filePath("case/log.prepare"));
    if (log.open(QIODevice::WriteOnly)) log.write(output);
    const QString target = QDir(parentDir).filePath("run_" + QUuid::createUuid().toString(QUuid::WithoutBraces));
    if (!QDir().rename(stage.filePath("case"), target)) {
        result.error = caseTr("The calculation storage folder could not be created. Check its location and permissions.");
        return result;
    }
    result.caseDir = target;
    return result;
}

} // namespace vortia
