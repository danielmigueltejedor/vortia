#pragma once
#include <QByteArray>
#include <QJsonObject>
#include <QString>
#include <functional>
#include <string>
#include <utility>
#include <vector>
namespace vortia {
/// Where compressible cases are built: the project's `cases` folder, `~/Library/Vortia/cases` on
/// macOS (off iCloud-managed folders), or the storage the user chose. Never a path with blanks,
/// which OpenFOAM's scripts cannot handle. Empty with `error` set when it cannot be used.
QString flowCaseStorage(const QString& projectRoot, const QString& caseStorage, QString* error);

// Update only runtime controls in a staged dictionary, preserving all saved fields.
// Returns an error message, or an empty string on success.
QString updateFlowRunControls(const QString& caseDir, const QJsonObject& solver,
                             const std::vector<std::pair<std::string,std::string>>& environment);
// Stage a content-addressed bundled backend without replacing legacy sources,
// executables, boundary dictionaries, mesh or saved fields. Empty = success.
QString stageFlowBackend(const QString& caseDir);

/// The `vortia` tool of the Rust engine shipped with this installation, or empty.
std::string bundledNativeEngine();
/// The Rust engine nozzle cases run on, or empty for OpenFOAM: `VORTIA_NATIVE_ENGINE` when set, else
/// the bundled tool when the "calculation/rustEngine" preference is on.
std::string nativeEnginePath();

/// The variables vortiaCentralFoam's build cache is keyed on: the engine, its build options and
/// its core libraries. Appended to the captured OpenFOAM environment for a compressible run.
std::vector<std::pair<std::string, std::string>> flowSolverRuntime(
    const std::vector<std::pair<std::string, std::string>>& environment, const std::string& installationFingerprint);

struct FlowCaseResult {
    QString caseDir;       ///< The new case, or empty on failure.
    QString error;         ///< What a person should read, when it failed.
    QByteArray diagnostic; ///< The mesher's own output, for the project's logs.
    bool cancelled{false};
};

/// Meshes the sketch with Gmsh and stages a complete compressible case, without any interface:
/// the desktop app wraps it in a progress dialog, vortia-engine in a job. `cancelled` is polled
/// while the mesher runs. On failure the diagnostic is also saved to `logs/mesh-generation.log`.
FlowCaseResult prepareFlowCase(const QString& sketchPath, const QJsonObject& configuration,
                               const QString& projectRoot,
                               const std::vector<std::pair<std::string, std::string>>& environment,
                               const QString& caseStorage, const std::function<bool()>& cancelled);
}
