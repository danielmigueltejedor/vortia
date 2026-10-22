#include "vortia/ui/JobController.hpp"

#include "vortia/core/Logger.hpp"

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
            callbacks.onOutput = [&](OutputStream stream, std::string_view chunk) {
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
                                              cancelToken_.get());
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
                summary = QStringLiteral("Finished at t=%1").arg(last.time, 0, 'g', 6);
            }
        }
    }

    running_.store(false);
    QMetaObject::invokeMethod(this, [this, success, summary]() {
        emit jobFinished(success, summary);
    }, Qt::QueuedConnection);
}

} // namespace vortia
