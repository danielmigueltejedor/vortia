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
#include <QStandardPaths>
#include <QDateTime>
#include <vector>
#include <tuple>
#include <cmath>
#include "vortia/solver/openfoam/Detector.hpp"
#include <QCryptographicHash>
#include <QTemporaryFile>
#include <QSaveFile>

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

QString stageFlowBackend(const QString& caseDir) {
    QFile marker(caseDir+"/.vortia-flow-case");
    if(!marker.open(QIODevice::ReadOnly)||marker.readAll().trimmed()!="vortia-axisymmetric-flow-v1")
        return caseTr("Only a generated flow case can receive the bundled backend.");
    QCryptographicHash hash(QCryptographicHash::Sha256);
    std::vector<std::pair<QString,QByteArray>> files;
    for(const auto& name:QStringList{"vortiaCentralFoam.C","ConservativeUpdate.H","ConvergencePolicy.H","ConvergenceMonitor.H","centralCourantNo.H","createFieldRefs.H","createFields.H","directionInterpolate.H","readFluxScheme.H","setRDeltaT.H","fixedRhoFvPatchScalarField.H","Make/files","Make/options","README.md","LICENSE.md"}) {
        QFile f(QString(":/vortia/openfoam/solver/")+name);
        if(!f.open(QIODevice::ReadOnly))return f.errorString();
        auto bytes=f.readAll();hash.addData(name.toUtf8());hash.addData(QByteArray(1,'\0'));hash.addData(bytes);
        files.emplace_back(name,std::move(bytes));
    }
    const auto revision=QString::fromLatin1(hash.result().toHex());
    const auto parent=caseDir+"/solver-backends",destination=parent+"/"+revision;
    if(!QDir().mkpath(parent))return caseTr("Cannot stage the bundled flow backend.");
    if(!QFileInfo::exists(destination)) {
        QTemporaryDir stage(parent+"/.stage-XXXXXX");
        if(!stage.isValid()||!QDir().mkpath(stage.filePath("Make")))return caseTr("Cannot stage the bundled flow backend.");
        for(const auto& [name,bytes]:files) {
            QFile f(stage.filePath(name));
            if(!f.open(QIODevice::WriteOnly)||f.write(bytes)!=bytes.size())return f.errorString();
        }
        if(!QDir().rename(stage.path(),destination))return caseTr("Cannot stage the bundled flow backend.");
        stage.setAutoRemove(false);
    } else {
        // Refuse a modified cache instead of silently trusting its name.
        for(const auto& [name,bytes]:files) {
            QFile f(destination+"/"+name);
            if(!f.open(QIODevice::ReadOnly)||f.readAll()!=bytes)return caseTr("The staged flow backend was modified; use a new mesh revision.");
        }
    }
    QSaveFile selected(caseDir+"/.vortia-solver-revision");
    if(!selected.open(QIODevice::WriteOnly)||selected.write(revision.toUtf8()+"\n")!=65||!selected.commit())return selected.errorString();
    return {};
}

QString updateFlowRunControls(const QString& caseDir,const QJsonObject& solver,
        const std::vector<std::pair<std::string,std::string>>& environment) {
    const double iterations=solver.value("iterations").toDouble(-1);
    const double interval=solver.value("writeInterval").toDouble(-1);
    const double courant=solver.value("maxCo").toDouble(-1);
    const double diagnostic=solver.value("logInterval").toDouble(10);
    if(!std::isfinite(diagnostic)||diagnostic<1||diagnostic>1000||std::floor(diagnostic)!=diagnostic)return caseTr("Invalid diagnostic output interval.");
    if(!std::isfinite(iterations)||!std::isfinite(interval)||!std::isfinite(courant)||
       iterations<1||iterations>200000||std::floor(iterations)!=iterations||
       interval<1||interval>iterations||std::floor(interval)!=interval||courant<=0||courant>.5)
        return caseTr("Invalid iteration limit, output interval or Courant number.");
    const auto c=solver.value("convergence").toObject();
    if(solver.contains("convergence")&&!solver.value("convergence").isObject())return caseTr("Invalid convergence criteria.");
    const std::vector<std::tuple<QString,double,double,double,bool>> defaults{
        {"interval",50,1,10000,true},{"minimumIterations",500,1,200000,true},{"stableSamples",5,2,100,true},
        {"massTolerance",.001,1e-8,.01,false},{"fieldTolerance",.001,1e-8,.01,false},
        {"equationTolerance",1e-5,1e-8,1e-3,false},{"absoluteMassTolerance",1e-9,1e-15,1e-6,false}};
    QString convergenceText="{ enabled "+QString(c.value("enabled").toBool(true)?"true":"false")+"; ";
    for(const auto& [key,fallback,low,high,integer]:defaults) {
        if(c.contains(key)&&!c.value(key).isDouble())return caseTr("Invalid convergence criteria.");
        const auto v=c.value(key).toDouble(fallback);
        if(!std::isfinite(v)||v<low||v>high||(integer&&std::floor(v)!=v))return caseTr("Invalid convergence criteria.");
        convergenceText+=key+" "+QString::number(v,'g',16)+"; ";
    }
    if((c.contains("enabled")&&!c.value("enabled").isBool())||c.value("minimumIterations").toDouble(500)<c.value("interval").toDouble(50))return caseTr("Invalid convergence criteria.");
    convergenceText+="}";
    QFile marker(caseDir+"/.vortia-flow-case");
    if(!marker.open(QIODevice::ReadOnly)||marker.readAll().trimmed()!="vortia-axisymmetric-flow-v1")
        return caseTr("Only a generated flow case can be resumed with these controls.");
    QFile original(caseDir+"/system/controlDict");
    if(!original.open(QIODevice::ReadOnly))return original.errorString();
    const auto bytes=original.readAll();original.close();
    QTemporaryFile staged(caseDir+"/system/.vortia-controls-XXXXXX");
    if(!staged.open()||staged.write(bytes)!=bytes.size()||!staged.flush())return staged.errorString();
    staged.close();
    const auto executable=openfoam::Detector::resolveExecutable("foamDictionary",environment);
    if(!executable)return caseTr("OpenFOAM foamDictionary is required to update calculation controls.");
    QProcessEnvironment captured;
    for(const auto& [key,value]:environment)captured.insert(QString::fromStdString(key),QString::fromStdString(value));
    for(const auto& entry:std::vector<std::pair<QString,QString>>{
        {"endTime",QString::number(iterations,'g',15)},{"writeInterval",QString::number(interval,'g',15)},
        {"maxCo",QString::number(courant,'g',15)},{"vortiaLogInterval",QString::number(diagnostic,'g',15)},{"vortiaConvergence",convergenceText}}) {
        QProcess command;command.setProcessEnvironment(captured);command.setWorkingDirectory(caseDir);
        command.start(QString::fromStdString(executable->string()),
            {staged.fileName(),"-entry",entry.first,"-set",entry.second});
        if(!command.waitForFinished(10000)||command.exitStatus()!=QProcess::NormalExit||command.exitCode()!=0) {
            command.kill();command.waitForFinished(1000);
            return caseTr("Calculation controls could not be updated; the existing case was preserved.")+"\n"+QString::fromUtf8(command.readAllStandardError());
        }
    }
    if(!staged.open())return staged.errorString();
    const auto updated=staged.readAll();
    if(updated.isEmpty())return caseTr("The updated calculation dictionary is empty.");
    QSaveFile output(caseDir+"/system/controlDict");
    if(!output.open(QIODevice::WriteOnly)||output.write(updated)!=updated.size()||!output.commit())return output.errorString();
    return {};
}

std::vector<std::pair<std::string, std::string>> flowSolverRuntime(
        const std::vector<std::pair<std::string, std::string>>& environment, const std::string& installation) {
    QJsonObject runtime{{"installation", QString::fromStdString(installation)}};
    QString libraries;
    for (const auto& [key, value] : environment) {
        if (key == "WM_PROJECT_DIR" || key == "WM_PROJECT_VERSION" || key == "WM_OPTIONS" || key == "WM_CXX"
            || key == "WM_LABEL_SIZE" || key == "WM_PRECISION_OPTION" || key == "FOAM_LIBBIN")
            runtime.insert(QString::fromStdString(key), QString::fromStdString(value));
        if (key == "FOAM_LIBBIN") libraries = QString::fromStdString(value);
    }
    for (const auto* name : {"libOpenFOAM", "libfiniteVolume", "libfluidThermophysicalModels"})
        for (const auto* extension : {".dylib", ".so"}) {
            QFileInfo library(QDir(libraries).filePath(QString::fromLatin1(name) + extension));
            if (library.exists())
                runtime.insert(library.fileName(), QString("%1:%2:%3").arg(library.canonicalFilePath())
                    .arg(library.size()).arg(library.lastModified().toMSecsSinceEpoch()));
        }
    const auto key = QCryptographicHash::hash(QJsonDocument(runtime).toJson(QJsonDocument::Compact),
                                              QCryptographicHash::Sha256).toHex();
    const auto cache = QDir(QStandardPaths::writableLocation(QStandardPaths::CacheLocation)).filePath("flow-engines");
    return {{"VORTIA_SOLVER_RUNTIME_KEY", key.toStdString()}, {"VORTIA_SOLVER_CACHE_DIR", cache.toStdString()}};
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
