#pragma once
#include <QJsonObject>
#include <QString>
#include <optional>
class QSettings;
class QWidget;
namespace vortia {
struct ApplicationPreferences {
    QString language="system",theme="light",density="comfortable",backendRoot,caseStorage;
    /// Linux: "nodalia" draws Nodalia's window controls in the header; "system" leaves the
    /// window's frame and buttons to the desktop (GNOME, KDE…). Applies after restarting.
    QString windowControls="nodalia";
    int interfaceScale=100,defaultProcesses=0,checkpointInterval=500,iterationLimit=50000,consoleLines=10000;
    double zoomSensitivity=1,snapSpacing=1;
    double cfdZoomSensitivity=1,cfdPanSensitivity=1;
    bool snapToGrid=true,showGrid=true,showDimensions=true,rememberLayout=true;
    bool showToolLabels=true;
    bool automaticMeshPreview=true;
    bool checkForUpdatesAutomatically=true;
    /// Run the nozzle cases on Vortia's own Rust engine instead of OpenFOAM (experimental).
    bool rustEngine=false;
    bool showConsoleOnRun=false,showAssessmentOnRun=true,automaticPreview=true;
    static ApplicationPreferences load(QSettings&);
    void save(QSettings&) const;
    QJsonObject flowDefaults() const;
};
std::optional<ApplicationPreferences> editApplicationPreferences(QWidget*,const ApplicationPreferences&,bool calculationRunning);
}
