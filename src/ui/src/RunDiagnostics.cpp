#include "vortia/ui/RunDiagnostics.hpp"
#include <QCoreApplication>
#include <QFile>
#include <QJsonDocument>
#include <QSaveFile>
#include <algorithm>
#include <cmath>

namespace vortia {
namespace {
QString tr(const char* text){return QCoreApplication::translate("vortia::RunDiagnostics",text);}
bool knownCode(const QString& code){return QStringList{"positivity","non-finite","mesh","missing-tool","memory","solver"}.contains(code);}
}
QJsonObject diagnoseRunFailure(openfoam::DivergenceKind kind,const QString& summary,
    const QString& evidence,const QString& phase,double iteration) {
    const auto text=summary+"\n"+evidence;
    QString code="solver";
    if(kind==openfoam::DivergenceKind::NonPositiveUpdate || text.contains("No positive conservative update after 30 reductions"))code="positivity";
    else if(kind==openfoam::DivergenceKind::NonFinite || text.contains("Floating point exception",Qt::CaseInsensitive))code="non-finite";
    else if(text.contains("bad_alloc")||text.contains("out of memory",Qt::CaseInsensitive)||text.contains("Cannot allocate memory",Qt::CaseInsensitive))code="memory";
    else if(text.contains("cannot resolve '")||text.contains("not found in OpenFOAM PATH"))code="missing-tool";
    else if(phase=="mesh")code="mesh";
    return QJsonObject{{"schema",1},{"status","failed"},{"code",code},{"summary",summary.left(4096)},
        {"evidence",evidence.right(16384)},{"phase",phase},{"iteration",std::isfinite(iteration)?iteration:0},
        {"rejectedEnergy",text.contains("Vortia rejected explicit energy")||text.contains("Vortia rejected diffused energy")}};
}
bool saveRunDiagnostic(const QString& caseDir,const QJsonObject& report) {
    if(caseDir.isEmpty())return false;
    QSaveFile file(caseDir+"/vortia-run-diagnostic.json");
    const auto bytes=QJsonDocument(report).toJson();
    return file.open(QIODevice::WriteOnly)&&file.write(bytes)==bytes.size()&&file.commit();
}
QJsonObject loadRunDiagnostic(const QString& caseDir) {
    if(caseDir.isEmpty())return {};
    QFile file(caseDir+"/vortia-run-diagnostic.json");
    if(file.open(QIODevice::ReadOnly)&&file.size()<=65536) {
        const auto report=QJsonDocument::fromJson(file.readAll()).object();
        if(report.value("schema").toInt()==1) {
            if(report.value("status")=="failed"&&knownCode(report.value("code").toString()))return report;
            // A later successful/cancelled/running attempt supersedes an earlier fatal log.
            if(QStringList{"success","cancelled","running"}.contains(report.value("status").toString()))return {};
        }
    }
    // Older versions only kept logs. Inspect the latest pipeline step, not an old failure.
    QFile log(caseDir+"/log.vortia");if(!log.open(QIODevice::ReadOnly))return {};
    log.seek(std::max<qint64>(0,log.size()-65536));auto tail=QString::fromUtf8(log.readAll());
    const auto start=tail.lastIndexOf("===== Vortia step:");if(start>=0)tail=tail.mid(start);
    if(tail.contains("No positive conservative update after 30 reductions"))
        return diagnoseRunFailure(openfoam::DivergenceKind::NonPositiveUpdate,{},tail,"solver",0);
    return {};
}
bool offersFreshFlowStart(const QJsonObject& d) {
    return d.value("status")=="failed"&&(d.value("code")=="positivity"||d.value("code")=="non-finite");
}
std::optional<QJsonObject> conservativeRestartConfiguration(const QJsonObject& cfg) {
    const auto solver=cfg.value("solver").toObject();const auto co=solver.value("maxCo").toDouble(-1);
    if(!std::isfinite(co)||co<.001||co>.5)return std::nullopt;
    auto next=cfg;auto controls=solver;
    controls.insert("maxCo",std::max(.001,std::min(.02,co*.25)));
    next.insert("solver",controls);return next;
}
QString runDiagnosticHtml(const QJsonObject& d,double courant) {
    const auto code=d.value("code").toString();QString title,why,steps;
    auto step=[&](const QString& text){steps+="<li>"+text.toHtmlEscaped()+"</li>";};
    if(code=="positivity"||code=="non-finite") {
        title=tr("Numerical instability · review the starting state");
        why=code=="positivity"?tr("The solver could not keep density and thermal energy admissible after repeatedly reducing the local step. This run has not converged."):
            tr("The solver reported non-finite values. Saved checkpoints require their own checks before reuse.");
        if(d.value("rejectedEnergy").toBool())why+=" "+tr("The log explicitly reports a rejected energy update; it does not identify the responsible cell or boundary.");
        auto configuration=conservativeRestartConfiguration(QJsonObject{{"solver",QJsonObject{{"maxCo",courant}}}});
        if(configuration) {
            const auto next=configuration->value("solver").toObject().value("maxCo").toDouble();
            if(next<courant)step(tr("In Calculation, try reducing the acoustic Courant limit from %1 to %2. This is a controlled retry, not a guarantee of convergence.").arg(courant,0,'g',4).arg(next,0,'g',4));
            else step(tr("The Courant limit is already at the supported minimum. Investigate initialization, boundaries, mesh and backend behavior instead of reducing it further."));
        }
        step(tr("Use Prepare fresh restart to create a new case from the configured initial values. The failed case and its results are retained; no calculation starts automatically."));
        step(tr("Review Physics > Boundary conditions: pressures are absolute Pa and temperatures are K. Check inlet/outlet assignments, reservoir values, gas properties and the initial pressure and temperature in Calculation. Do not change the prescribed physics just to suppress the error."));
        step(tr("Review Mesh near the throat, sharp corners and region cuts. Check quality and abrupt size changes. A passed mesh check alone does not establish solution stability."));
        step(tr("If it repeats, compare a fresh run with one CPU process and save the diagnostic. Serial execution is a diagnostic comparison, not a cure. The experimental backend may need a numerical fix; more iterations will not repair this failure."));
        why+=" "+tr("A rejected candidate does not by itself prove that an earlier saved checkpoint is invalid. MPI_ABORT stops the remaining parallel workers after a failure; rank numbers do not establish the cause.");
    } else if(code=="mesh") {
        title=tr("Mesh preparation failed");why=tr("The calculation did not obtain a usable checked mesh.");
        step(tr("Open Mesh, review the last mesh-check messages and highlighted geometry. Repair open contours, intersecting cuts or invalid regions, then generate a new revision."));
        step(tr("Keep the earlier case for comparison. A flow calculation cannot resolve a mesh-generation error."));
    } else if(code=="missing-tool") {
        title=tr("Calculation tool unavailable");why=tr("A required executable could not be resolved in the selected engine.");
        step(tr("In Preferences > Storage and OpenFOAM, leave the engine path empty to use the integrated engine, then restart Vortia. If using an external installation, verify that it contains the required tool."));
    } else if(code=="memory") {
        title=tr("Memory allocation failed");why=tr("The log reports an allocation failure.");
        step(tr("Try fewer CPU processes and a smaller mesh for diagnosis. Close other memory-intensive work, then retry with a new case while retaining earlier results."));
    } else {
        title=tr("Calculation stopped · review the diagnostic");why=tr("The available messages do not establish a specific cause. Keep the complete log rather than guessing a physical correction.");
        step(tr("Open the detailed console and inspect the first specific error before the MPI shutdown messages. Save the diagnostic and review the relevant calculation, physics or mesh controls."));
    }
    return "<h3>"+title.toHtmlEscaped()+"</h3><p>"+why.toHtmlEscaped()+"</p><h4>"+tr("What to do next").toHtmlEscaped()+"</h4><ol>"+steps+"</ol>";
}
} // namespace vortia
