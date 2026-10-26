#include "vortia/ui/JobController.hpp"

#include "vortia/core/Logger.hpp"
#include "vortia/solver/openfoam/Detector.hpp"
#include "vortia/ui/ResultFiles.hpp"

#include <filesystem>
#include <chrono>
#include <QJsonObject>
#include <QJsonDocument>
#include <QJsonArray>

#include <QMetaObject>

namespace vortia {

JobController::JobController(QObject* parent) : QObject(parent) {}

JobController::~JobController() {
    cancel();
    if (worker_.joinable())
        worker_.join();
}

bool JobController::start(const JobSpec& spec) {
    if (running_.load())
        return false;
    // A completed std::thread remains joinable. Reassigning it on the next
    // project/run would call std::terminate.
    if (worker_.joinable()) worker_.join();
    cancelToken_ = std::make_unique<CancellationToken>();
    cancelRequested_.store(false);
    running_.store(true);

    JobSpec copy = spec;
    worker_ = std::thread(&JobController::runJob, this, std::move(copy));
    return true;
}

void JobController::cancel() {
    cancelRequested_.store(true);
    if (cancelToken_)
        cancelToken_->cancel();
}

bool JobController::isRunning() const { return running_.load(); }

void JobController::runJob(JobSpec spec) {
    try {
    PosixProcessRunner processes;
    openfoam::CaseRunner runner(processes, spec.environment);

    bool success = true;
    QString summary;

    openfoam::RunCallbacks meshCallbacks;
    meshCallbacks.onOutput = [this](OutputStream, std::string_view chunk) {
        const QString text=QString::fromUtf8(chunk.data(),static_cast<qsizetype>(chunk.size()));
        QMetaObject::invokeMethod(this,[this,text]() { emit outputReceived(text); },Qt::QueuedConnection);
    };
    auto meshStep = runner.runMeshing(spec.caseDir, spec.caps, meshCallbacks, cancelToken_.get());
    if (!meshStep) {
        success = false;
        summary = QString::fromStdString(meshStep.error().describe());
    } else {
        if (!meshStep->process.succeeded()) {
            success = false;
            summary = QStringLiteral("%1 failed (exit %2)").arg(QString::fromStdString(meshStep->name)).arg(meshStep->process.exitCode);
        } else {
            if (spec.meshOnly) {
                ProcessRequest request;
                request.executable="foamToVTK";
                request.arguments={"-latestTime","-no-fields"};
                request.workingDirectory=spec.caseDir; request.environment=spec.environment;
                const auto executable=openfoam::Detector::resolveExecutable("foamToVTK",spec.environment);
                if (executable) request.executable=executable->string();
                const auto result=processes.run(request,cancelToken_.get(),nullptr);
                const bool converted=result && result->succeeded();
                const auto vtm=converted ? latestVtmFile(QString::fromStdString((spec.caseDir/"VTK").string())) : QString{};
                running_.store(false);
                QMetaObject::invokeMethod(this,[this,vtm,converted]() {
                    if (!vtm.isEmpty()) emit resultsReady(vtm);
                    emit jobFinished(converted,converted ? tr("Mesh checked and exported; no CFD calculation was run.") : tr("Mesh export failed."));
                },Qt::QueuedConnection);
                return;
            }
            openfoam::SolverProgress last;
            auto lastProgress=std::chrono::steady_clock::now()-std::chrono::milliseconds(100);
            openfoam::RunCallbacks callbacks;
            callbacks.onOutput = [&](OutputStream /*stream*/, std::string_view chunk) {
                const QString text = QString::fromUtf8(chunk.data(),
                                                      static_cast<qsizetype>(chunk.size()));
                QMetaObject::invokeMethod(this, [this, text]() {
                    emit outputReceived(text);
                }, Qt::QueuedConnection);
            };
            QString lastAssessment;
            std::optional<openfoam::ConvergenceSample> lastConvergence;
            QJsonArray pendingMonitors;
            auto lastMonitors=std::chrono::steady_clock::now();
            const auto publishMonitors=[&](){if(pendingMonitors.isEmpty())return;const auto json=QString::fromUtf8(QJsonDocument(pendingMonitors).toJson(QJsonDocument::Compact));pendingMonitors={};QMetaObject::invokeMethod(this,[this,json](){emit monitorSamples(json);},Qt::QueuedConnection);lastMonitors=std::chrono::steady_clock::now();};
            callbacks.onMonitors=[&](const std::vector<openfoam::MonitorSample>& samples){
                for(const auto& sample:samples)pendingMonitors.append(QJsonArray{sample.coordinate,sample.value,QString::fromStdString(sample.series),sample.session});
                if(pendingMonitors.size()>=4096||std::chrono::steady_clock::now()-lastMonitors>=std::chrono::milliseconds(200))publishMonitors();
            };
            callbacks.onProgress = [&](const openfoam::SolverProgress& p) {
                last = p;
                if(p.convergence && (!lastConvergence || p.convergence->iteration!=lastConvergence->iteration ||
                    p.convergence->status!=lastConvergence->status)) {
                    lastConvergence=p.convergence;
                    const auto& c=*p.convergence;
                    const QString report=QString::fromUtf8(QJsonDocument(QJsonObject{
                        {"status",QString::fromStdString(c.status)},{"iteration",c.iteration},{"checkedIteration",c.checkedIteration},{"sampled",c.sampled},{"automatic",c.automatic},
                        {"globalMass",c.globalMass},{"localMass",c.localMass},{"fieldChange",c.fieldChange},
                        {"equationError",c.equationError},{"equationTolerance",c.equationTolerance},{"massTolerance",c.massTolerance},{"fieldTolerance",c.fieldTolerance},
                        {"stableSamples",c.stableSamples},{"requiredSamples",c.requiredSamples},{"interval",c.interval}
                    }).toJson(QJsonDocument::Compact));
                    if(report!=lastAssessment) {
                        lastAssessment=report;
                        QMetaObject::invokeMethod(this,[this,report](){emit assessmentUpdated(report);},Qt::QueuedConnection);
                    }
                }
                const auto now=std::chrono::steady_clock::now();
                if(!p.diverged && now-lastProgress<std::chrono::milliseconds(100))return;
                lastProgress=now;
                QString res;
                for (const auto& r : p.residuals)
                    res += QString::fromStdString(r.field) + "=" +
                           QString::number(r.final, 'g', 6) + "  ";
                QMetaObject::invokeMethod(this, [this, p, res]() {
                    emit progressUpdated(p.time, p.courantMax, res);
                }, Qt::QueuedConnection);
            };

            auto solverStep = runner.runSolver(spec.caseDir, spec.setup, spec.caps, callbacks,
                                              cancelToken_.get(), spec.solverOverride,spec.processes);
            publishMonitors();
            if (!solverStep) {
                success = false;
                summary = QString::fromStdString(solverStep.error().describe());
            } else if (solverStep->diverged) {
                success = false;
                summary = solverStep->divergenceKind==openfoam::DivergenceKind::NonPositiveUpdate
                    ? tr("Numerical instability: no admissible density/energy update after 30 local-step reductions. This run is not converged. Review initial values, boundary conditions and mesh before restarting from a valid state.")
                    : tr("Solver stopped: %1").arg(QString::fromStdString(solverStep->divergenceReason));
            } else if (solverStep->process.cancelled) {
                success = false;
                summary = tr("Calculation cancelled. Resume from the latest saved checkpoint.");
            } else if (!solverStep->process.succeeded()) {
                success = false;
                summary = QStringLiteral("Solver failed (exit %1)").arg(solverStep->process.exitCode);
            } else {
                double finalT = solverStep->finalTime > 0 ? solverStep->finalTime : last.time;
                summary = (spec.solverOverride=="vortiaCentralFoam"
                    ? (lastConvergence && lastConvergence->status=="converged"
                        ? tr("Numerical convergence reached at iteration %1. Results saved.")
                        : tr("Stopped at iteration %1 without numerical convergence. Results are provisional; continue the calculation or revise the setup."))
                    : tr("Finished at t=%1")).arg(finalT, 0, 'g', 6);
                // Convert results to VTK for visualization (best-effort).
                if(spec.automaticPreview) {
                ProcessRequest vtkReq;
                vtkReq.executable = "foamToVTK";
                vtkReq.arguments = {"-latestTime"};
                vtkReq.workingDirectory = spec.caseDir;
                vtkReq.environment = spec.environment;
                auto vtkPath = openfoam::Detector::resolveExecutable("foamToVTK", spec.environment);
                if (vtkPath) {
                    vtkReq.executable = vtkPath.value().string();
                    auto vtkResult = processes.run(vtkReq, nullptr, nullptr);
                    if (vtkResult && vtkResult->succeeded()) {
                        // Select the ROOT .vtm (directly in VTK/), not nested
                        // boundary patches. Read actual times from the series metadata.
                        const auto latest = latestVtmFile(QString::fromStdString((spec.caseDir/"VTK").string()));
                        if (!latest.isEmpty()) {
                            QMetaObject::invokeMethod(this, [this, latest]() {
                                emit resultsReady(latest);
                            }, Qt::QueuedConnection);
                        } else {
                            QMetaObject::invokeMethod(this, [this]() {
                                emit outputReceived(QStringLiteral("foamToVTK produced no .vtm file\n"));
                            }, Qt::QueuedConnection);
                        }
                    } else {
                        QString err = vtkResult ? QString::fromStdString(vtkResult->stdErr)
                                                : QStringLiteral("foamToVTK not run");
                        QMetaObject::invokeMethod(this, [this, err]() {
                            emit outputReceived(QStringLiteral("foamToVTK failed: %1\n").arg(err));
                        }, Qt::QueuedConnection);
                    }
                }
                } else {
                    QMetaObject::invokeMethod(this,[this](){emit outputReceived(tr("Native results saved. Use View latest result to prepare visualization.\n"));},Qt::QueuedConnection);
                }

            }
        }
    }

    running_.store(false);
    QMetaObject::invokeMethod(this, [this, success, summary]() {
        emit jobFinished(success, summary);
    }, Qt::QueuedConnection);
    } catch (const std::exception& e) {
        running_.store(false);
        const QString message = QString::fromStdString(std::string("exception: ") + e.what());
        QMetaObject::invokeMethod(this, [this, message]() {
            emit jobFinished(false, message);
        }, Qt::QueuedConnection);
    } catch (...) {
        running_.store(false);
        QMetaObject::invokeMethod(this, [this]() {
            emit jobFinished(false, QStringLiteral("unknown exception"));
        }, Qt::QueuedConnection);
    }
}

} // namespace vortia
