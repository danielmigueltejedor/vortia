#pragma once
#include <QJsonObject>
#include <QString>
#include <optional>
class QSettings;
class QWidget;
namespace vortia {
struct ApplicationPreferences {
    QString language="system",theme="dark",density="comfortable",backendRoot,caseStorage;
    int interfaceScale=100,defaultProcesses=0,checkpointInterval=500,iterationLimit=50000,consoleLines=10000;
    double zoomSensitivity=1,snapSpacing=1;
    double cfdZoomSensitivity=1,cfdPanSensitivity=1;
    bool snapToGrid=true,showGrid=true,showDimensions=true,rememberLayout=true;
    bool showToolLabels=true,showStageNumbers=true;
    bool automaticMeshPreview=true;
    bool showConsoleOnRun=false,showAssessmentOnRun=true,automaticPreview=true;
    static ApplicationPreferences load(QSettings&);
    void save(QSettings&) const;
    QJsonObject flowDefaults() const;
};
std::optional<ApplicationPreferences> editApplicationPreferences(QWidget*,const ApplicationPreferences&,bool calculationRunning);
}
