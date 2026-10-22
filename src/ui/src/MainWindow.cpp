#include "vortia/ui/MainWindow.hpp"

#include "vortia/core/Logger.hpp"
#include "vortia/core/Project.hpp"
#include "vortia/geometry/StlReader.hpp"
#include "vortia/interop/fluent/FluentCffImporter.hpp"
#include "vortia/interop/fluent/FluentFormatDetector.hpp"
#include "vortia/interop/fluent/FluentMeshReader.hpp"
#include "vortia/interop/fluent/FluentToOpenFoam.hpp"
#include "vortia/mesh/BlockMeshDict.hpp"
#include "vortia/physics/CaseSetupJson.hpp"
#include "vortia/solver/openfoam/CaseWriter.hpp"
#include "vortia/solver/openfoam/Detector.hpp"
#include "vortia/ui/ThemeManager.hpp"

#include <QMenuBar>
#include <QStyle>
#include <QSize>

#include <QAction>
#include <QApplication>
#include <QDockWidget>
#include <QFileDialog>
#include <QFileInfo>
#include <QInputDialog>
#include <QLineEdit>
#include <QMenu>
#include <QMessageBox>
#include <QStatusBar>
#include <QToolBar>
#include <QLabel>

#include <filesystem>
#include <fstream>
#include <memory>
#include <sstream>

namespace vortia {

namespace {

CaseSetup makeCavity() {
    CaseSetup s;
    s.domain = BoxDomain::fromExtents({0, 0, 0}, {0.1, 0.1, 0.01}).value();
    s.mesh.cells = {20, 20, 1};
    s.mesh.patches = {{
        {BoxDomain::XMin, "fixedWalls", PatchKind::Wall},
        {BoxDomain::XMax, "fixedWalls2", PatchKind::Wall},
        {BoxDomain::YMin, "fixedWalls3", PatchKind::Wall},
        {BoxDomain::YMax, "movingWall", PatchKind::Wall},
        {BoxDomain::ZMin, "frontAndBack", PatchKind::Empty},
        {BoxDomain::ZMax, "frontAndBack2", PatchKind::Empty},
    }};
    s.material.name = "air";
    s.material.nu = KinematicViscosity(0.001);
    s.boundaries = {
        {"fixedWalls", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"fixedWalls2", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"fixedWalls3", VelocityBc::NoSlip, {}, PressureBc::ZeroGradient, 0.0},
        {"movingWall", VelocityBc::FixedValue, {1.0, 0.0, 0.0}, PressureBc::ZeroGradient, 0.0},
        {"frontAndBack", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
        {"frontAndBack2", VelocityBc::Empty, {}, PressureBc::Empty, 0.0},
    };
    s.time = {0.0, 0.5, 0.001, 100, false, 1.0};
    s.solver = SolverApp::IcoFoam;
    return s;
}

} // namespace

MainWindow::MainWindow(QWidget* parent) : QMainWindow(parent) {
    settings_ = std::make_unique<QSettings>(QStringLiteral("Vortia"), QStringLiteral("Vortia"));
    setup_ = makeCavity();

    setWindowTitle(tr("Vortia — Engineering the Flow"));
    resize(1280, 800);

    buildMenusAndToolbar();
    buildDocks();

    jobController_ = new JobController(this);
    connect(jobController_, &JobController::outputReceived, console_, &ConsolePanel::appendSolverOutput);
    connect(jobController_, &JobController::jobFinished, this, &MainWindow::onJobFinished);
    connect(jobController_, &JobController::resultsReady, this, &MainWindow::onResultsReady);
    connect(jobController_, &JobController::progressUpdated, this,
            [this](double time, double coMax, const QString& residuals) {
                setProjectState(QStringLiteral("t=%1  Co_max=%2  %3")
                                    .arg(time, 0, 'g', 6)
                                    .arg(coMax, 0, 'g', 4)
                                    .arg(residuals));
            });

    properties_->setSetup(setup_);
    connect(properties_, &PropertiesPanel::setupChanged, this, &MainWindow::onApplySetup);
    connect(properties_, &PropertiesPanel::validationReported, this,
            &MainWindow::onValidationReport);

    connect(explorer_, &ProjectExplorerPanel::nodeActivated, this, &MainWindow::onNodeActivated);
    explorer_->retranslate();

    loadSettings();
    updateViewport();
    setProjectState(tr("New untitled project — default cavity template (Re=100)."));
}

MainWindow::~MainWindow() = default;

void MainWindow::buildMenusAndToolbar() {
    auto* fileMenu = menuBar()->addMenu(tr("File"));
    auto* newProject = fileMenu->addAction(tr("New Project"), this, &MainWindow::onNewProject, QKeySequence::New);
    auto* openProject = fileMenu->addAction(tr("Open Project…"), this, &MainWindow::onOpenProject, QKeySequence::Open);
    auto* openFoam = fileMenu->addAction(tr("Open OpenFOAM Case…"), this, &MainWindow::onOpenExistingCase);
    auto* saveProject = fileMenu->addAction(tr("Save Project"), this, &MainWindow::onSaveProject, QKeySequence::Save);
    fileMenu->addSeparator();
    auto* importStl = fileMenu->addAction(tr("Import STL…"), this, &MainWindow::onImportStl);
    fileMenu->addAction(tr("Import ANSYS Fluent…"), this, &MainWindow::onImportFluent);
    fileMenu->addSeparator();
    fileMenu->addAction(tr("Exit"), this, &QWidget::close, QKeySequence::Quit);

    auto* simMenu = menuBar()->addMenu(tr("Simulation"));
    simMenu->addAction(tr("Detect OpenFOAM Installation"), this, &MainWindow::onDetectOpenFoam);
    runAction_ = simMenu->addAction(tr("Run Simulation"), this, &MainWindow::onRun, QKeySequence(tr("Ctrl+R")));
    cancelAction_ = simMenu->addAction(tr("Cancel Simulation"), this, &MainWindow::onCancel);
    cancelAction_->setEnabled(false);

    auto* viewMenu = menuBar()->addMenu(tr("View"));
    viewMenu->setObjectName(QStringLiteral("vortia-view-menu"));
    themeAction_ = viewMenu->addAction(tr("Toggle Dark/Light Theme"), this, &MainWindow::onToggleTheme);

    auto* helpMenu = menuBar()->addMenu(tr("Help"));
    helpMenu->addAction(tr("About Vortia"), this, &MainWindow::onAbout);

    // Retain explicit action pointers: relying on menu indices broke Save when
    // "Open OpenFOAM Case" was inserted before it.
    auto* tb = addToolBar(tr("Workspace"));
    tb->setObjectName(QStringLiteral("vortia-main-toolbar"));
    tb->setMovable(false);
    tb->setFloatable(false);
    tb->setIconSize(QSize(18, 18));
    tb->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    newProject->setIcon(style()->standardIcon(QStyle::SP_FileIcon));
    openProject->setIcon(style()->standardIcon(QStyle::SP_DialogOpenButton));
    saveProject->setIcon(style()->standardIcon(QStyle::SP_DialogSaveButton));
    runAction_->setIcon(style()->standardIcon(QStyle::SP_MediaPlay));
    cancelAction_->setIcon(style()->standardIcon(QStyle::SP_MediaStop));
    tb->addAction(newProject);
    tb->addAction(openProject);
    tb->addAction(saveProject);
    tb->addSeparator();
    tb->addAction(openFoam);
    tb->addAction(importStl);
    tb->addSeparator();
    tb->addAction(runAction_);
    tb->addAction(cancelAction_);
    tb->addSeparator();

    auto* fieldLabel = new QLabel(tr("FIELD"), this);
    fieldLabel->setObjectName(QStringLiteral("vortia-field-label"));
    tb->addWidget(fieldLabel);
    fieldCombo_ = new QComboBox(this);
    fieldCombo_->setObjectName(QStringLiteral("vortia-field-selector"));
    fieldCombo_->setMinimumWidth(92);
    fieldCombo_->setToolTip(tr("Select a field to display on the current CFD result."));
    fieldCombo_->addItems({QStringLiteral("p"), QStringLiteral("U"), QStringLiteral("|U|"),
                           QStringLiteral("Ux"), QStringLiteral("Uy"), QStringLiteral("Uz"),
                           QStringLiteral("T")});
    tb->addWidget(fieldCombo_);
    connect(fieldCombo_, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            &MainWindow::onFieldChanged);

    tb->addSeparator();
    auto* viewXY = tb->addAction(tr("XY"));
    auto* viewXZ = tb->addAction(tr("XZ"));
    auto* viewYZ = tb->addAction(tr("YZ"));
    auto* viewIso = tb->addAction(tr("3D"));
    auto* viewReset = tb->addAction(tr("Fit"));
    viewXY->setToolTip(tr("Look along Z at the XY plane"));
    viewXZ->setToolTip(tr("Look along Y at the XZ plane"));
    viewYZ->setToolTip(tr("Look along X at the YZ plane"));
    viewIso->setToolTip(tr("Isometric view"));
    viewReset->setToolTip(tr("Fit the geometry into the view"));
    connect(viewXY, &QAction::triggered, this, [this]() { viewport_->setViewXY(); });
    connect(viewXZ, &QAction::triggered, this, [this]() { viewport_->setViewXZ(); });
    connect(viewYZ, &QAction::triggered, this, [this]() { viewport_->setViewYZ(); });
    connect(viewIso, &QAction::triggered, this, [this]() { viewport_->setViewIsometric(); });
    connect(viewReset, &QAction::triggered, this, [this]() { viewport_->resetCamera(); });

    statusLabel_ = new QLabel(this);
    statusLabel_->setObjectName(QStringLiteral("vortia-status-text"));
    statusBar()->addWidget(statusLabel_, 1);
}

void MainWindow::buildDocks() {
    setDockOptions(QMainWindow::AnimatedDocks | QMainWindow::AllowTabbedDocks
                   | QMainWindow::AllowNestedDocks);
    viewport_ = new VtkViewport(this);
    setCentralWidget(viewport_);

    auto* explorerDock = new QDockWidget(tr("Project Explorer"), this);
    explorerDock->setObjectName(QStringLiteral("vortia-dock-project"));
    explorerDock->setAllowedAreas(Qt::LeftDockWidgetArea | Qt::RightDockWidgetArea);
    explorer_ = new ProjectExplorerPanel(this);
    explorerDock->setWidget(explorer_);
    addDockWidget(Qt::LeftDockWidgetArea, explorerDock);

    auto* propsDock = new QDockWidget(tr("Properties"), this);
    propsDock->setObjectName(QStringLiteral("vortia-dock-properties"));
    propsDock->setAllowedAreas(Qt::LeftDockWidgetArea | Qt::RightDockWidgetArea);
    properties_ = new PropertiesPanel(this);
    propsDock->setWidget(properties_);
    addDockWidget(Qt::RightDockWidgetArea, propsDock);

    auto* consoleDock = new QDockWidget(tr("Simulation Console"), this);
    consoleDock->setObjectName(QStringLiteral("vortia-dock-console"));
    consoleDock->setAllowedAreas(Qt::BottomDockWidgetArea | Qt::TopDockWidgetArea);
    consoleDock->setFeatures(QDockWidget::DockWidgetClosable |
                             QDockWidget::DockWidgetMovable |
                             QDockWidget::DockWidgetFloatable);
    console_ = new ConsolePanel(this);
    consoleDock->setWidget(console_);
    addDockWidget(Qt::BottomDockWidgetArea, consoleDock);
    resizeDocks({explorerDock, propsDock}, {230, 295}, Qt::Horizontal);
    resizeDocks({consoleDock}, {155}, Qt::Vertical);

    if (auto* viewMenu = findChild<QMenu*>(QStringLiteral("vortia-view-menu"))) {
        viewMenu->addSeparator();
        viewMenu->addAction(explorerDock->toggleViewAction());
        viewMenu->addAction(propsDock->toggleViewAction());
        viewMenu->addAction(consoleDock->toggleViewAction());
        viewMenu->addSeparator();
        auto* resetWorkspace = viewMenu->addAction(tr("Reset Workspace Layout"));
        connect(resetWorkspace, &QAction::triggered, this,
                [this, explorerDock, propsDock, consoleDock]() {
                    explorerDock->setFloating(false);
                    propsDock->setFloating(false);
                    consoleDock->setFloating(false);
                    addDockWidget(Qt::LeftDockWidgetArea, explorerDock);
                    addDockWidget(Qt::RightDockWidgetArea, propsDock);
                    addDockWidget(Qt::BottomDockWidgetArea, consoleDock);
                    explorerDock->show();
                    propsDock->show();
                    consoleDock->show();
                    resizeDocks({explorerDock, propsDock}, {230, 295}, Qt::Horizontal);
                    resizeDocks({consoleDock}, {155}, Qt::Vertical);
                });
    }
}

void MainWindow::loadSettings() {
    const QString theme = settings_->value(QStringLiteral("theme"), QStringLiteral("dark")).toString();
    ThemeManager::apply(*qApp, ThemeManager::fromName(theme));
    const QByteArray geometry = settings_->value(QStringLiteral("window/geometry")).toByteArray();
    if (!geometry.isEmpty())
        restoreGeometry(geometry);
    const QByteArray state = settings_->value(QStringLiteral("window/state")).toByteArray();
    if (!state.isEmpty())
        restoreState(state);
}

void MainWindow::saveSettings() {
    settings_->setValue(QStringLiteral("theme"),
                       ThemeManager::toName(ThemeManager::fromName(
                           settings_->value(QStringLiteral("theme"), QStringLiteral("dark")).toString())));
    settings_->setValue(QStringLiteral("window/geometry"), saveGeometry());
    settings_->setValue(QStringLiteral("window/state"), saveState());
    settings_->sync();
}

void MainWindow::updateViewport() {
    viewport_->showBoxDomain(&setup_.domain);
    viewport_->setDarkTheme(ThemeManager::fromName(
        settings_->value(QStringLiteral("theme"), QStringLiteral("dark")).toString()) ==
        ThemeManager::Theme::Dark);
}

void MainWindow::onNewProject() {
    if (dirty_) {
        const auto r = QMessageBox::question(this, tr("Unsaved changes"),
                                             tr("Discard current project and create a new one?"));
        if (r != QMessageBox::Yes)
            return;
    }
    projectRoot_.clear();
    setup_ = makeCavity();
    properties_->setSetup(setup_);
    updateViewport();
    dirty_ = false;
    setProjectState(tr("New untitled project — default cavity template (Re=100)."));
}

void MainWindow::onOpenProject() {
    const QString dir = QFileDialog::getExistingDirectory(this, tr("Open Vortia Project"));
    if (dir.isEmpty())
        return;
    auto project = Project::open(dir.toStdString());
    if (!project) {
        // Not a Vortia project — maybe it's an OpenFOAM case directory.
        if (std::filesystem::exists(dir.toStdString() + "/system/controlDict")) {
            existingCaseDir_ = dir.toStdString();
            useExistingCase_ = true;
            setProjectState(tr("Opened existing OpenFOAM case: %1").arg(dir));
            console_->appendSolverOutput(tr("Opened existing OpenFOAM case: %1\n").arg(dir));
            return;
        }
        QMessageBox::warning(this, tr("Open failed"), QString::fromStdString(project.error().describe()));
        return;
    }
    projectRoot_ = project->rootDir();
    if (!project->setupJson().empty()) {
        auto loaded = caseSetupFromJson(project->setupJson());
        if (loaded)
            setup_ = loaded.value();
        else
            QMessageBox::warning(this, tr("Open warning"),
                                 tr("Case setup could not be fully restored: %1")
                                     .arg(QString::fromStdString(loaded.error().describe())));
    }
    if (!project->backendFingerprint().empty())
        ofInstallation_ = openfoam::Installation{}; // cleared; re-detect on run
    properties_->setSetup(setup_);
    updateViewport();
    dirty_ = false;
    setProjectState(tr("Opened project: %1").arg(QString::fromStdString(project->name())));
}

void MainWindow::onSaveProject() {
    if (projectRoot_.empty()) {
        const QString dir = QFileDialog::getExistingDirectory(this, tr("Choose project location"));
        if (dir.isEmpty())
            return;
        const QString name = QInputDialog::getText(this, tr("Project name"), tr("Name:"), QLineEdit::Normal,
                                                   QStringLiteral("cavity"));
        if (name.isEmpty())
            return;
        auto project = Project::create((dir.toStdString() + "/" + name.toStdString()), name.toStdString());
        if (!project) {
            QMessageBox::warning(this, tr("Save failed"), QString::fromStdString(project.error().describe()));
            return;
        }
        projectRoot_ = project->rootDir();
    }

    auto project = Project::open(projectRoot_);
    if (!project) {
        QMessageBox::warning(this, tr("Save failed"), QString::fromStdString(project.error().describe()));
        return;
    }
    project->setName(projectRoot_.filename().string());
    project->setSetupJson(caseSetupToJson(setup_));
    if (hasOfInstallation_)
        project->setBackendFingerprint(ofInstallation_.fingerprint());
    if (dirty_ == false) { /* state untouched */ }
    if (std::filesystem::exists(projectRoot_ / "case" / openfoam::CaseWriter::kMarkerFile))
        project->setCaseState(Project::CaseState::Configured);
    if (auto saved = project->save(); !saved) {
        QMessageBox::warning(this, tr("Save failed"), QString::fromStdString(saved.error().describe()));
        return;
    }
    dirty_ = false;
    setProjectState(tr("Saved project: %1").arg(QString::fromStdString(project->name())));
}

void MainWindow::onImportStl() {
    const QString path = QFileDialog::getOpenFileName(this, tr("Import STL"), QString(),
                                                      tr("STL files (*.stl *.STL)"));
    if (path.isEmpty())
        return;
    auto mesh = StlReader::read(path.toStdString());
    if (!mesh) {
        QMessageBox::warning(this, tr("Import failed"), QString::fromStdString(mesh.error().describe()));
        return;
    }
    viewport_->showTriangleMesh(&*mesh);
    setProjectState(tr("Imported STL: %1 (%2 triangles)")
                       .arg(QFileInfo(path).fileName())
                       .arg(static_cast<qlonglong>(mesh->triangles().size())));
}

void MainWindow::onImportFluent() {
    const QString path = QFileDialog::getOpenFileName(
        this, tr("Import ANSYS Fluent"), QString(),
        tr("Fluent files (*.msh *.msh.h5 *.cas *.cas.h5 *.dat *.dat.h5 *.gz);;All files (*)"));
    if (path.isEmpty())
        return;

    using namespace fluent;
    const auto format = detectFormat(path.toStdString());
    console_->appendSolverOutput(
        tr("Detected Fluent format: %1\n").arg(QString::fromStdString(std::string(toString(format)))));

    if (format == FluentFormat::CaseHdf5 || format == FluentFormat::DataHdf5) {
        auto result = FluentCffImporter::import(path.toStdString());
        if (!result.ok) {
            QMessageBox::warning(this, tr("Import failed"), tr("Could not open CFF file."));
            return;
        }
        viewport_->showDataSet(result.data);
        // Build a summary.
        QString summary = tr("Imported Fluent CFF (visualization only).\n");
        for (const auto& n : result.report.notes)
            summary += QString::fromStdString(n) + "\n";
        for (const auto& w : result.report.warnings)
            summary += tr("warning: %1\n").arg(QString::fromStdString(w));
        QMessageBox::information(this, tr("Fluent import"), summary);
        setProjectState(tr("Imported Fluent CFF (visualization only)."));
        return;
    }

    if (format == FluentFormat::MeshAscii) {
        auto mesh = readTraditionalMesh(path.toStdString());
        if (!mesh) {
            QMessageBox::warning(this, tr("Import failed"), QString::fromStdString(mesh.error().describe()));
            return;
        }
        const auto report = assessCompatibility(*mesh, format, path.toStdString());

        // Show mesh in viewport as a point cloud of nodes is not useful; instead
        // show the converted OpenFOAM mesh's... we don't have VTK unstructured
        // here. Show node count in dialog.
        QString summary;
        summary += tr("Format: %1\n").arg(QString::fromStdString(std::string(toString(format))));
        summary += tr("Nodes: %1\nFaces: %2\nCells: %3\nZones: %4\n")
                       .arg(static_cast<qlonglong>(mesh->nodes.size()))
                       .arg(static_cast<qlonglong>(mesh->faces.size()))
                       .arg(static_cast<qlonglong>(mesh->cells.size()))
                       .arg(static_cast<qlonglong>(mesh->zones.size()));
        summary += tr("Dimension: %1D\n").arg(mesh->dimension);
        summary += tr("Compatibility: %1\n").arg(QString::fromStdString(std::string(toString(report.level))));
        for (const auto& inc : report.incompatibilities)
            summary += tr("  - %1: %2 (%3)\n")
                           .arg(QString::fromStdString(inc.component))
                           .arg(QString::fromStdString(inc.fluentValue))
                           .arg(QString::fromStdString(inc.reason));
        for (const auto& n : report.notes)
            summary += tr("  note: %1\n").arg(QString::fromStdString(n));

        // Offer conversion.
        const auto reply = QMessageBox::question(
            this, tr("Convert to OpenFOAM?"),
            summary + "\n" + tr("Convert this mesh to an OpenFOAM case now?"),
            QMessageBox::Yes | QMessageBox::No);
        if (reply == QMessageBox::Yes) {
            const QString outDir = QFileDialog::getExistingDirectory(this, tr("Output case directory"));
            if (!outDir.isEmpty()) {
                CompatibilityReport r = report;
                auto written = convertToOpenFoam(*mesh, outDir.toStdString(), &r);
                if (written) {
                    console_->appendSolverOutput(
                        tr("Converted Fluent mesh to OpenFOAM: %1 files written to %2\n")
                            .arg(static_cast<int>(written->size()))
                            .arg(outDir));
                    for (const auto& p : *written)
                        console_->appendSolverOutput("  " + QString::fromStdString(p.string()) + "\n");
                } else {
                    QMessageBox::warning(this, tr("Conversion failed"),
                                         QString::fromStdString(written.error().describe()));
                }
            }
        }
        setProjectState(tr("Imported Fluent mesh (%1 nodes, %2 cells).")
                           .arg(static_cast<qlonglong>(mesh->nodes.size()))
                           .arg(static_cast<qlonglong>(mesh->cells.size())));
        return;
    }

    if (format == FluentFormat::MeshHdf5) {
        QMessageBox::information(
            this, tr("Fluent import"),
            tr("CFF .msh.h5 mesh detected. Direct calculation-grade import is planned (P1). "
               "Open it via this dialog for visualization using the CFF reader if a .cas.h5 "
               "companion is available, or convert with fluentMeshToFoam when installed."));
        return;
    }

    if (format == FluentFormat::Compressed) {
        QMessageBox::information(this, tr("Fluent import"),
                                 tr("Compressed Fluent file detected. Decompress it first, "
                                    "then re-import."));
        return;
    }

    QMessageBox::warning(this, tr("Fluent import"),
                        tr("Unrecognised or unsupported Fluent file: %1").arg(path));
}

void MainWindow::onOpenExistingCase() {
    const QString dir = QFileDialog::getExistingDirectory(this, tr("Open OpenFOAM Case"));
    if (dir.isEmpty())
        return;
    const std::filesystem::path caseDir = dir.toStdString();
    if (!std::filesystem::exists(caseDir / "system" / "controlDict")) {
        QMessageBox::warning(this, tr("Not an OpenFOAM case"),
                             tr("The selected directory has no system/controlDict."));
        return;
    }
    existingCaseDir_ = caseDir;
    useExistingCase_ = true;
    projectRoot_.clear();
    dirty_ = false;
    setProjectState(tr("Opened existing case: %1").arg(dir));
    console_->appendSolverOutput(tr("Opened existing OpenFOAM case: %1\n").arg(dir));
}

void MainWindow::onNodeActivated(int nodeId) {
    properties_->showNode(nodeId);
}

void MainWindow::onApplySetup(const CaseSetup& setup) {
    setup_ = setup;
    dirty_ = true;
    updateViewport();
}

void MainWindow::onValidationReport(const QString& text) {
    console_->appendSolverOutput(text + "\n");
}

void MainWindow::onRun() {
    try {
    ensureOpenFoamInstallation();
    if (!hasOfInstallation_) {
        QMessageBox::warning(this, tr("No OpenFOAM installation"),
                             tr("Vortia could not find an OpenFOAM installation.\n"
                                "Install OpenFOAM (Linux/WSL2) or configure "
                                "VORTIA_OPENFOAM_DIR, then retry."));
        return;
    }

    // Capture env if needed.
    if (ofEnvironment_.empty()) {
        PosixProcessRunner runner;
        auto env = openfoam::Detector::captureEnvironment(ofInstallation_, runner);
        if (!env) {
            QMessageBox::warning(this, tr("Environment capture failed"),
                                 QString::fromStdString(env.error().describe()));
            return;
        }
        ofEnvironment_ = env.value();
        ofCaps_ = openfoam::Detector::verifyCapabilities(ofInstallation_, ofEnvironment_);
    }

    if (!ofCaps_.hasBlockMesh) {
        QMessageBox::warning(this, tr("Backend incomplete"),
                             tr("The detected OpenFOAM installation lacks blockMesh. "
                                "Aborting."));
        return;
    }

    std::filesystem::path caseDir;
    std::string detectedSolver;
    if (useExistingCase_ && !existingCaseDir_.empty()) {
        caseDir = existingCaseDir_;
        console_->appendSolverOutput(tr("Using existing case: %1\n").arg(QString::fromStdString(caseDir.string())));
        // Detect the solver from controlDict.
        std::ifstream cd(caseDir / "system" / "controlDict");
        std::string line;
        while (std::getline(cd, line)) {
            auto pos = line.find("application");
            if (pos != std::string::npos) {
                std::istringstream iss(line.substr(pos));
                std::string key, val;
                iss >> key >> val;
                if (!val.empty()) {
                    detectedSolver = val;
                    if (!detectedSolver.empty() && detectedSolver.back() == ';')
                        detectedSolver.pop_back();
                }
                break;
            }
        }
    } else {
        caseDir = projectRoot_.empty() ? std::filesystem::temp_directory_path() / "vortia-case"
                                       : projectRoot_ / "case";
        auto issues = setup_.validate();
        for (const auto& i : issues) {
            if (i.severity == Severity::Error) {
                QMessageBox::warning(this, tr("Invalid setup"), QString::fromStdString(i.message));
                return;
            }
            console_->appendSolverOutput(QStringLiteral("warning: %1\n").arg(QString::fromStdString(i.message)));
        }
        auto written = openfoam::CaseWriter::write(caseDir, setup_, ofCaps_, {.overwrite = true});
        if (!written) {
            QMessageBox::warning(this, tr("Case generation failed"),
                                 QString::fromStdString(written.error().describe()));
            return;
        }
        console_->appendSolverOutput(tr("Wrote %1 case files to %2\n")
                                        .arg(static_cast<int>(written->size()))
                                        .arg(QString::fromStdString(caseDir.string())));
    }

    runAction_->setEnabled(false);
    cancelAction_->setEnabled(true);

    JobController::JobSpec spec;
    spec.caseDir = caseDir;
    spec.setup = setup_;
    spec.caps = ofCaps_;
    spec.environment = ofEnvironment_;
    spec.solverOverride = detectedSolver;
    jobController_->start(spec);
    } catch (const std::exception& e) {
        QMessageBox::warning(this, tr("Run failed"),
                             QString::fromStdString(std::string("exception: ") + e.what()));
    } catch (...) {
        QMessageBox::warning(this, tr("Run failed"), tr("unknown exception"));
    }
}

void MainWindow::onCancel() {
    jobController_->cancel();
    console_->appendSolverOutput(tr("Cancellation requested…\n"));
}

void MainWindow::onJobFinished(bool success, const QString& summary) {
    runAction_->setEnabled(true);
    cancelAction_->setEnabled(false);
    setProjectState(success ? tr("Run finished: %1").arg(summary)
                           : tr("Run failed: %1").arg(summary));
    console_->appendSolverOutput((success ? tr("SUCCESS: ") : tr("FAILED: ")) + summary + "\n");
}

void MainWindow::onResultsReady(const QString& vtmPath) {
    try {
        lastVtmPath_ = vtmPath;
        viewport_->loadVtmFile(vtmPath.toStdString(), fieldCombo_->currentText().toStdString());
        console_->appendSolverOutput(tr("Loaded results: %1\n").arg(vtmPath));
        setProjectState(tr("Results loaded."));
    } catch (const std::exception& e) {
        console_->appendSolverOutput(tr("Failed to load results: %1\n").arg(QString::fromStdString(e.what())));
    } catch (...) {
        console_->appendSolverOutput(tr("Failed to load results: unknown error\n"));
    }
}

void MainWindow::onFieldChanged(int /*index*/) {
    if (lastVtmPath_.isEmpty())
        return;
    viewport_->loadVtmFile(lastVtmPath_.toStdString(), fieldCombo_->currentText().toStdString());
}

void MainWindow::onToggleTheme() {
    const QString current = settings_->value(QStringLiteral("theme"), QStringLiteral("dark")).toString();
    const QString next = current == QStringLiteral("dark") ? QStringLiteral("light")
                                                           : QStringLiteral("dark");
    settings_->setValue(QStringLiteral("theme"), next);
    ThemeManager::apply(*qApp, ThemeManager::fromName(next));
    updateViewport();
}

void MainWindow::onDetectOpenFoam() {
    const auto installs = openfoam::Detector::scan();
    if (installs.empty()) {
        QMessageBox::information(this, tr("OpenFOAM detection"),
                                 tr("No OpenFOAM installation found.\n"
                                    "Set VORTIA_OPENFOAM_DIR or install OpenFOAM."));
        return;
    }
    // Prefer the first; let the user see the list in the console.
    ofInstallation_ = installs.front();
    hasOfInstallation_ = true;
    console_->appendSolverOutput(tr("Detected %1 installation(s):\n").arg(installs.size()));
    for (const auto& i : installs)
        console_->appendSolverOutput("  " + QString::fromStdString(i.fingerprint()) + "\n");
    console_->appendSolverOutput(tr("Using: %1\n").arg(QString::fromStdString(ofInstallation_.fingerprint())));
    ofEnvironment_.clear(); // force re-capture
}

void MainWindow::ensureOpenFoamInstallation() {
    if (hasOfInstallation_)
        return;
    const auto installs = openfoam::Detector::scan();
    if (!installs.empty()) {
        ofInstallation_ = installs.front();
        hasOfInstallation_ = true;
        ofEnvironment_.clear();
    }
}

void MainWindow::onAbout() {
    QMessageBox::about(this, tr("About Vortia"),
                       tr("Vortia — Engineering the Flow\n\n"
                          "Open-source CFD desktop application powered by OpenFOAM.\n"
                          "Version 0.1.0 (pre-MVP)\n\n"
                          "Original implementation. Vortia is not affiliated with ANSYS, Inc."));
}

void MainWindow::setProjectState(const QString& statusText) {
    statusLabel_->setText(statusText);
}

void MainWindow::closeEvent(QCloseEvent* event) {
    saveSettings();
    QMainWindow::closeEvent(event);
}

} // namespace vortia
