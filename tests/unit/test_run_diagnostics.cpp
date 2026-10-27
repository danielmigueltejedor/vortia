#include "vortia/ui/RunDiagnostics.hpp"
#include "vortia/ui/JobController.hpp"
#include <gtest/gtest.h>
#include <QTemporaryDir>
#include <QFile>
#include <QDir>
#include <QJsonArray>
#include <QJsonDocument>
#include <QElapsedTimer>
#include <QCoreApplication>
#include <QThread>

using namespace vortia;
namespace {
const QString failure="[5] Vortia rejected explicit energy\n[0] --> FOAM FATAL ERROR:\n[0] No positive conservative update after 30 reductions\nMPI_ABORT was invoked on rank 7\nprterun has exited";
void write(const QString& path,const QByteArray& bytes){QFile f(path);ASSERT_TRUE(f.open(QIODevice::WriteOnly));ASSERT_EQ(f.write(bytes),bytes.size());}
TEST(RunDiagnostics, ParallelAbortKeepsSpecificCauseAndGuidance) {
    const auto d=diagnoseRunFailure(openfoam::DivergenceKind::Fatal,"Solver failed",failure,"solver",82460);
    EXPECT_EQ(d.value("code"),"positivity");EXPECT_TRUE(d.value("rejectedEnergy").toBool());
    EXPECT_DOUBLE_EQ(d.value("iteration").toDouble(),82460);EXPECT_TRUE(offersFreshFlowStart(d));
    const auto html=runDiagnosticHtml(d,.1);EXPECT_TRUE(html.contains("0.02"));EXPECT_TRUE(html.contains("MPI_ABORT"));
    EXPECT_TRUE(html.contains("not a guarantee"));EXPECT_TRUE(html.contains("does not by itself prove"));
    EXPECT_FALSE(offersFreshFlowStart(diagnoseRunFailure(openfoam::DivergenceKind::None,"MPI_ABORT",{},"solver",0)));
}
TEST(RunDiagnostics, ControlledRestartPreservesPhysicsAndDoesNotIncreaseCourant) {
    const QJsonObject cfg{{"initial",QJsonObject{{"pressure",101325},{"temperature",300}}},
        {"boundaries",QJsonArray{QJsonObject{{"type","inlet"},{"pressure",3640000}}}},
        {"solver",QJsonObject{{"maxCo",.1},{"iterations",50000},{"processes",8},{"convergence",QJsonObject{{"enabled",true}}}}},
        {"activeCase","old-case"}};
    const auto next=conservativeRestartConfiguration(cfg);ASSERT_TRUE(next);
    EXPECT_EQ(next->value("initial"),cfg.value("initial"));EXPECT_EQ(next->value("boundaries"),cfg.value("boundaries"));
    auto expected=cfg.value("solver").toObject();expected.insert("maxCo",.02);EXPECT_EQ(next->value("solver"),expected);
    EXPECT_EQ(next->value("activeCase"),cfg.value("activeCase"));
    for(double co:{.001,.002,.01,.02,.1,.5}) {
        const auto r=conservativeRestartConfiguration(QJsonObject{{"solver",QJsonObject{{"maxCo",co}}}});ASSERT_TRUE(r);
        EXPECT_LE(r->value("solver").toObject().value("maxCo").toDouble(),co);
        EXPECT_GE(r->value("solver").toObject().value("maxCo").toDouble(),.001);
    }
    EXPECT_FALSE(conservativeRestartConfiguration({}));
    EXPECT_FALSE(conservativeRestartConfiguration(QJsonObject{{"solver",QJsonObject{{"maxCo",.9}}}}));
}
TEST(RunDiagnostics, PersistenceAndLegacyReplayDoNotResurrectAnOldFailure) {
    QTemporaryDir dir;write(dir.filePath("log.vortia"),("===== Vortia step: parallel flow (mpirun) =====\n"+failure).toUtf8());
    EXPECT_EQ(loadRunDiagnostic(dir.path()).value("code"),"positivity");
    const auto d=diagnoseRunFailure(openfoam::DivergenceKind::NonPositiveUpdate,{},failure,"solver",12);
    ASSERT_TRUE(saveRunDiagnostic(dir.path(),d));EXPECT_EQ(loadRunDiagnostic(dir.path()),d);
    for(const auto* state:{"running","success","cancelled"}) {
        ASSERT_TRUE(saveRunDiagnostic(dir.path(),QJsonObject{{"schema",1},{"status",state}}));
        EXPECT_TRUE(loadRunDiagnostic(dir.path()).isEmpty());
    }
    ASSERT_TRUE(QFile::remove(dir.filePath("vortia-run-diagnostic.json")));
    write(dir.filePath("log.vortia"),(failure+"\n===== Vortia step: parallel flow (mpirun) =====\nTime = 100\nEnd\n").toUtf8());
    EXPECT_TRUE(loadRunDiagnostic(dir.path()).isEmpty());EXPECT_FALSE(saveRunDiagnostic({},d));
}
TEST(RunDiagnostics, UnknownAndToolFailuresDoNotInventPhysicalCauses) {
    EXPECT_EQ(diagnoseRunFailure(openfoam::DivergenceKind::None,"cannot resolve 'wmake'",{},"solver",0).value("code"),"missing-tool");
    EXPECT_EQ(diagnoseRunFailure(openfoam::DivergenceKind::None,"std::bad_alloc",{},"solver",0).value("code"),"memory");
    const auto generic=diagnoseRunFailure(openfoam::DivergenceKind::None,"exit 1","MPI_ABORT","solver",0);
    EXPECT_EQ(generic.value("code"),"solver");EXPECT_FALSE(offersFreshFlowStart(generic));
    EXPECT_TRUE(runDiagnosticHtml(generic,.1).contains("do not establish a specific cause"));
}
TEST(RunDiagnostics, WorkerPublishesAndPersistsDiagnosisBeforeReturningToIdle) {
    QTemporaryDir dir;const auto bin=dir.filePath("tools");ASSERT_TRUE(QDir().mkpath(bin));
    for(const auto* name:{"blockMesh","checkMesh","fakeFoam"}) {
        const auto path=bin+"/"+name;
        write(path,QByteArray("#!/bin/sh\n")+(QString(name)=="fakeFoam"?
            "echo 'Vortia rejected explicit energy'\necho '[7] No positive conservative update after 30 reductions' >&2\necho 'MPI_ABORT' >&2\nexit 1\n":"exit 0\n"));
        ASSERT_TRUE(QFile::setPermissions(path,QFileDevice::ReadOwner|QFileDevice::WriteOwner|QFileDevice::ExeOwner));
    }
    JobController controller;QJsonObject diagnostic;bool finished=false;
    QObject::connect(&controller,&JobController::jobFinished,[&](bool success,const QString&){EXPECT_FALSE(success);finished=true;});
    QObject::connect(&controller,&JobController::diagnosticReady,[&](const QString& json){diagnostic=QJsonDocument::fromJson(json.toUtf8()).object();});
    JobController::JobSpec spec;spec.caseDir=dir.path().toStdString();spec.environment={{"PATH",bin.toStdString()}};
    spec.caps.hasBlockMesh=true;spec.caps.hasCheckMesh=true;spec.solverOverride="fakeFoam";
    ASSERT_TRUE(controller.start(spec));QElapsedTimer timer;timer.start();
    while((!finished||diagnostic.isEmpty())&&timer.elapsed()<5000){QCoreApplication::processEvents();QThread::msleep(1);}
    ASSERT_TRUE(finished);EXPECT_EQ(diagnostic.value("code"),"positivity");EXPECT_TRUE(diagnostic.value("rejectedEnergy").toBool());
    EXPECT_EQ(loadRunDiagnostic(dir.path()),diagnostic);EXPECT_FALSE(controller.isRunning());
}
} // namespace
