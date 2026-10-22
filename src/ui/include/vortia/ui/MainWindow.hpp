#pragma once

#include "vortia/core/Process.hpp"
#include "vortia/physics/Physics.hpp"
#include "vortia/solver/openfoam/Installation.hpp"
#include "vortia/ui/ConsolePanel.hpp"
#include "vortia/ui/JobController.hpp"
#include "vortia/ui/ProjectExplorerPanel.hpp"
#include "vortia/ui/PropertiesPanel.hpp"
#include "vortia/visualization/VtkViewport.hpp"

#include <QLabel>
#include <QMainWindow>
#include <QSettings>
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
    void onImportStl();
    void onImportFluent();
    void onNodeActivated(int nodeId);
    void onApplySetup(const CaseSetup& setup);
    void onValidationReport(const QString& text);
    void onRun();
    void onCancel();
    void onJobFinished(bool success, const QString& summary);
    void onToggleTheme();
    void onDetectOpenFoam();
    void onAbout();

private:
    void buildMenusAndToolbar();
    void buildDocks();
    void loadSettings();
    void saveSettings();
    void updateViewport();
    void setProjectState(const QString& statusText);
    void ensureOpenFoamInstallation();
    CaseSetup defaultCavitySetup() const;

    // Project state
    std::filesystem::path projectRoot_;
    CaseSetup setup_;
    std::vector<std::pair<std::string, std::string>> ofEnvironment_;
    openfoam::Capabilities ofCaps_{};
    openfoam::Installation ofInstallation_{};
    bool hasOfInstallation_{false};
    bool dirty_{false};

    // UI
    VtkViewport* viewport_{nullptr};
    ProjectExplorerPanel* explorer_{nullptr};
    PropertiesPanel* properties_{nullptr};
    ConsolePanel* console_{nullptr};
    JobController* jobController_{nullptr};

    QAction* runAction_{nullptr};
    QAction* cancelAction_{nullptr};
    QAction* themeAction_{nullptr};
    QLabel* statusLabel_{nullptr};
    std::unique_ptr<QSettings> settings_;
};

} // namespace vortia
