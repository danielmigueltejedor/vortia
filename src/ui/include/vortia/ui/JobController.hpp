#pragma once

#include "vortia/core/Process.hpp"
#include "vortia/physics/Physics.hpp"
#include "vortia/solver/openfoam/Installation.hpp"
#include "vortia/solver/openfoam/CaseRunner.hpp"

#include <QObject>
#include <QString>
#include <atomic>
#include <memory>
#include <thread>

namespace vortia {

/// Runs the OpenFOAM pipeline (blockMesh → solver) off the GUI thread.
/// Owns a worker thread; safe to start/cancel repeatedly.
class JobController : public QObject {
    Q_OBJECT

public:
    struct JobSpec {
        std::filesystem::path caseDir;
        CaseSetup setup;
        openfoam::Capabilities caps;
        std::vector<std::pair<std::string, std::string>> environment;
    };

    explicit JobController(QObject* parent = nullptr);
    ~JobController() override;

    /// Returns false if a job is already running.
    bool start(const JobSpec& spec);
    void cancel();
    bool isRunning() const;

signals:
    void outputReceived(const QString& text);
    void progressUpdated(double time, double courantMax, const QString& residuals);
    void resultsReady(const QString& vtmPath);
    void jobFinished(bool success, const QString& summary);

private:
    void runJob(JobSpec spec);

    std::thread worker_;
    std::atomic<bool> running_{false};
    std::unique_ptr<CancellationToken> cancelToken_;
    std::atomic<bool> cancelRequested_{false};
};

} // namespace vortia
