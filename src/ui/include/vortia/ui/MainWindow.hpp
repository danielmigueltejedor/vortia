#pragma once

#include "vortia/core/Process.hpp"
#include "vortia/physics/Physics.hpp"
#include "vortia/solver/openfoam/Installation.hpp"
#include "vortia/ui/ConsolePanel.hpp"
#include "vortia/ui/JobController.hpp"
#include "vortia/ui/ProjectExplorerPanel.hpp"
#include "vortia/ui/PropertiesPanel.hpp"
#include "vortia/ui/SketchEditorDialog.hpp"
#include "vortia/visualization/VtkViewport.hpp"

#include <QComboBox>
#include <QLabel>
#include <QMainWindow>
#include <QSettings>
#include <QTabBar>
#include <QTabWidget>
#include <vector>
#include <memory>

namespace vortia {

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget* parent = nullptr);
    ~MainWindow() override;

protected:
    void closeEvent(QCloseEvent* event) override;

private slots:
    void onNewProject();
    void onOpenProject();
    void onSaveProject();
    void onOpenExistingCase();
    void onImportStl();
    void onImportFluent();
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
    void onDetectOpenFoam();
    void onAbout();

private:
    void buildMenusAndToolbar();
    void buildDocks();
    void addProjectTab(const QString& name, CaseSetup setup,
                       const std::filesystem::path& root = {},
                       const std::filesystem::path& caseDir = {});
    void activateProjectTab(int index);
    void closeProjectTab(int index);
    void storeCurrentTab();
    void refreshProjectTabTitle();
    void loadSettings();
    void saveSettings();
    void updateViewport();
    void setProjectState(const QString& statusText);
    void ensureOpenFoamInstallation();
    CaseSetup defaultCavitySetup() const;

    struct ProjectDocument {
        QString name;
        CaseSetup setup;
        Sketch2D sketch;
        NozzleDimensions nozzleDimensions;
        std::filesystem::path root;
        std::filesystem::path externalCase;
        bool existingCase{false};
        bool modified{false};
        QString resultVtm;
    };
    std::vector<ProjectDocument> documents_;
    int activeProjectTab_{-1};
    int untitledCount_{0};

    // Active document state, synchronized when changing tabs.
    std::filesystem::path projectRoot_;
    std::filesystem::path existingCaseDir_;
    bool useExistingCase_{false};
    CaseSetup setup_;
    std::vector<std::pair<std::string, std::string>> ofEnvironment_;
    openfoam::Capabilities ofCaps_{};
    openfoam::Installation ofInstallation_{};
    bool hasOfInstallation_{false};
    bool dirty_{false};

    // UI
    QTabBar* projectTabs_{nullptr};
    QTabWidget* workspaceTabs_{nullptr};
    SketchEditorDialog* sketchEditor_{nullptr};
    VtkViewport* viewport_{nullptr};
    ProjectExplorerPanel* explorer_{nullptr};
    PropertiesPanel* properties_{nullptr};
    ConsolePanel* console_{nullptr};
    JobController* jobController_{nullptr};

    QAction* runAction_{nullptr};
    QAction* cancelAction_{nullptr};
    QAction* themeAction_{nullptr};
    QComboBox* fieldCombo_{nullptr};
    QLabel* statusLabel_{nullptr};
    QString lastVtmPath_;
    std::unique_ptr<QSettings> settings_;
};

} // namespace vortia
