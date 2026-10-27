#pragma once

#include "vortia/core/Process.hpp"
#include "vortia/physics/Physics.hpp"
#include "vortia/solver/openfoam/Installation.hpp"
#include "vortia/ui/ConsolePanel.hpp"
#include "vortia/ui/ApplicationPreferences.hpp"
#include "vortia/ui/JobController.hpp"
#include "vortia/ui/ProjectExplorerPanel.hpp"
#include "vortia/ui/PropertiesPanel.hpp"
#include "vortia/ui/SketchEditorDialog.hpp"
#include "vortia/visualization/VtkViewport.hpp"

#include <QComboBox>
#include <QJsonObject>
#include <QLabel>
#include <QMainWindow>
#include <QSettings>
#include <QTabBar>
#include <QTabWidget>
#include <vector>
#include <memory>

class QTemporaryDir;
class QStackedWidget;

namespace vortia {
class RunMonitor;
class MeshRegionsWidget;
class FlowSetupPanel;

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget* parent = nullptr);
    ~MainWindow() override;

protected:
    void closeEvent(QCloseEvent* event) override;
    void showEvent(QShowEvent*) override;
    bool eventFilter(QObject*, QEvent*) override;

private slots:
    void onNewProject();
    void onOpenProject();
    void onSaveProject();
    void onOpenExistingCase();
    void onImportStl();
    void onImportFluent();
    void configureFlow(int page = 0);
    void onGenerateFlowMesh();
    bool ensureFlowCase(bool force = false);
    bool ensureFlowEnvironment();
    QString flowFingerprint(bool includeRunControls = false) const;
    void onOpenResults();
    void onViewLatestResult();
    void onNodeActivated(int nodeId);
    void onApplySetup(const CaseSetup& setup);
    void onValidationReport(const QString& text);
    void onRun();
    void onCancel();
    void onJobFinished(bool success, const QString& summary);
    void onResultsReady(const QString& vtmPath);
    void onFieldChanged(int index);
    void onToggleTheme();
    void onPreferences();
    void onFindTool();
    void onDetectOpenFoam();
    void onAbout();

private:
    friend struct WorkspaceTestAccess;
    bool openProjectDirectory(const QString& directory);
    bool saveProject();
    bool projectSwitchLocked() const;
    void setJobUiRunning(bool running);
    void buildMenusAndToolbar();
    void buildDocks();
    void activateWorkflowStage(int stage);
    void setContextToolsVisible(bool);
    void updateWorkflowInspector();
    void rebuildMeshWorkspace();
    void rebuildPhysicsWorkspace();
    void rebuildCalculationControls();
    void clearFlowEditors();
    void applyFlowEdits(const QJsonObject&, bool physics);
    bool usesSketchFlow() const;
    void updateNozzleNotice();
    void showFlowAssessment(const QJsonObject& report);
    void addProjectTab(const QString& name, CaseSetup setup,
                       const std::filesystem::path& root = {},
                       const std::filesystem::path& caseDir = {});
    void activateProjectTab(int index);
    void closeProjectTab(int index);
    void storeCurrentTab();
    void refreshProjectTabTitle();
    void loadSettings();
    void applyPreferences();
    void saveSettings();
    void updateViewport();
    void setProjectState(const QString& statusText);
    void ensureOpenFoamInstallation();
    void prepareDiagnosticRestart();
    void showRunDiagnostic(const QJsonObject& diagnostic);
    CaseSetup defaultCavitySetup() const;

    struct ProjectDocument {
        QString name;
        CaseSetup setup;
        Sketch2D sketch;
        SketchEditorDialog::EditHistory editHistory;
        NozzleDimensions nozzleDimensions;
        std::filesystem::path root;
        std::filesystem::path externalCase;
        bool existingCase{false};
        bool modified{false};
        QString resultVtm;
        QJsonObject flowConfig;
        std::shared_ptr<QTemporaryDir> temporaryCase;
    };
    std::vector<ProjectDocument> documents_;
    int activeProjectTab_{-1};
    int untitledCount_{0};

    // Active document state, synchronized when changing tabs.
    std::filesystem::path projectRoot_;
    std::filesystem::path existingCaseDir_;
    bool useExistingCase_{false};
    CaseSetup setup_;
    ApplicationPreferences preferences_;
    QJsonObject flowConfig_;
    std::vector<std::pair<std::string, std::string>> ofEnvironment_;
    openfoam::Capabilities ofCaps_{};
    openfoam::Installation ofInstallation_{};
    QString ofDetectionError_;
    bool hasOfInstallation_{false};
    bool dirty_{false};

    // UI
    QTabBar* projectTabs_{nullptr};
    QTabWidget* workspaceTabs_{nullptr};
    SketchEditorDialog* sketchEditor_{nullptr};
    VtkViewport* viewport_{nullptr};
    RunMonitor* runMonitor_{nullptr};
    ProjectExplorerPanel* explorer_{nullptr};
    PropertiesPanel* properties_{nullptr};
    ConsolePanel* console_{nullptr};
    JobController* jobController_{nullptr};

    QAction* runAction_{nullptr};
    QAction* newProjectAction_{nullptr};
    QAction* openProjectAction_{nullptr};
    QAction* openCaseAction_{nullptr};
    QAction* cancelAction_{nullptr};
    QAction* themeAction_{nullptr};
    void refreshFieldTools();
    std::vector<QAction*> fieldActions_;
    QComboBox* fieldCombo_{nullptr};
    QLabel* statusLabel_{nullptr};
    QTabWidget* propertiesTabs_{nullptr};
    QLabel* flowAssessment_{nullptr};
    QWidget* recoveryControls_{nullptr};
    QJsonObject runDiagnostic_;
    QStackedWidget* inspectorStack_{nullptr};
    QLabel* workflowSummary_{nullptr};
    QWidget* meshWorkspacePage_{nullptr};
    MeshRegionsWidget* meshWorkspace_{nullptr};
    QWidget* physicsWorkspacePage_{nullptr};
    FlowSetupPanel* physicsEditor_{nullptr};
    FlowSetupPanel* calculationEditor_{nullptr};
    QLabel* nozzleNotice_{nullptr};
    QString lastVtmPath_;
    std::unique_ptr<QSettings> settings_;
};

} // namespace vortia
