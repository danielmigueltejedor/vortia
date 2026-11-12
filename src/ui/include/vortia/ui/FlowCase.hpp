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
