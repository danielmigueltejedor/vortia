#include "vortia/ui/JobController.hpp"

#include "vortia/core/Logger.hpp"
#include "vortia/solver/openfoam/Detector.hpp"

#include <filesystem>

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

    auto meshStep = runner.runMeshing(spec.caseDir, spec.caps, {}, cancelToken_.get());
    if (!meshStep) {
        success = false;
        summary = QString::fromStdString(meshStep.error().describe());
    } else {
        if (!meshStep->process.succeeded()) {
            success = false;
            summary = QStringLiteral("blockMesh failed (exit %1)").arg(meshStep->process.exitCode);
        } else {
            openfoam::SolverProgress last;
            openfoam::RunCallbacks callbacks;
            callbacks.onOutput = [&](OutputStream /*stream*/, std::string_view chunk) {
                const QString text = QString::fromUtf8(chunk.data(),
                                                      static_cast<qsizetype>(chunk.size()));
                QMetaObject::invokeMethod(this, [this, text]() {
                    emit outputReceived(text);
                }, Qt::QueuedConnection);
            };
            callbacks.onProgress = [&](const openfoam::SolverProgress& p) {
                last = p;
                QString res;
                for (const auto& r : p.residuals)
                    res += QString::fromStdString(r.field) + "=" +
                           QString::number(r.final, 'g', 6) + "  ";
                QMetaObject::invokeMethod(this, [this, p, res]() {
                    emit progressUpdated(p.time, p.courantMax, res);
                }, Qt::QueuedConnection);
            };

            auto solverStep = runner.runSolver(spec.caseDir, spec.setup, spec.caps, callbacks,
                                              cancelToken_.get(), spec.solverOverride);
            if (!solverStep) {
                success = false;
                summary = QString::fromStdString(solverStep.error().describe());
            } else if (solverStep->diverged) {
                success = false;
                summary = QStringLiteral("Solver diverged: %1")
                              .arg(QString::fromStdString(solverStep->divergenceReason));
            } else if (!solverStep->process.succeeded()) {
                success = false;
                summary = QStringLiteral("Solver failed (exit %1)").arg(solverStep->process.exitCode);
            } else {
                double finalT = solverStep->finalTime > 0 ? solverStep->finalTime : last.time;
                summary = QStringLiteral("Finished at t=%1").arg(finalT, 0, 'g', 6);
                // Convert results to VTK for visualization (best-effort).
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
                        // boundary patches. Pick the one with the highest
                        // trailing time token.
                        std::filesystem::path latest;
                        std::filesystem::path vtkDir = spec.caseDir / "VTK";
                        std::error_code ec;
                        if (std::filesystem::is_directory(vtkDir, ec)) {
                            std::filesystem::directory_iterator it(vtkDir, ec);
                            std::filesystem::directory_iterator end;
                            while (it != end) {
                                std::error_code ec2;
                                auto entry = *it;
                                ++it;
                                if (!entry.is_regular_file(ec2))
                                    continue;
                                if (entry.path().extension() != ".vtm")
                                    continue;
                                if (latest.empty() || entry.path().filename() > latest.filename())
                                    latest = entry.path();
                            }
                        }
                        if (!latest.empty()) {
                            QMetaObject::invokeMethod(this, [this, latest]() {
                                emit resultsReady(QString::fromStdString(latest.string()));
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
            }
        }
    }

    running_.store(false);
    QMetaObject::invokeMethod(this, [this, success, summary]() {
        emit jobFinished(success, summary);
    }, Qt::QueuedConnection);
    } catch (const std::exception& e) {
        running_.store(false);
        QMetaObject::invokeMethod(this, [this, e]() {
            emit jobFinished(false, QString::fromStdString(std::string("exception: ") + e.what()));
        }, Qt::QueuedConnection);
    } catch (...) {
        running_.store(false);
        QMetaObject::invokeMethod(this, [this]() {
            emit jobFinished(false, QStringLiteral("unknown exception"));
        }, Qt::QueuedConnection);
    }
}

} // namespace vortia
