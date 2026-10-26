#include "vortia/ui/FlowSetup.hpp"
#include "vortia/ui/LineProfile.hpp"
#include "vortia/ui/FieldTools.hpp"
#include <QActionGroup>
#include <QSaveFile>
#include <QCryptographicHash>
#include <QJsonDocument>
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
#include "vortia/ui/SketchEditorDialog.hpp"
#include "vortia/ui/ResultFiles.hpp"

#include <QMenuBar>
#include <initializer_list>
#include <QWidget>
#include <QSizePolicy>
#include <QToolButton>
#include <QTabWidget>
#include <QTabBar>
#include <QSignalBlocker>
#include <QFileInfo>
#include <QFile>
#include <QDir>
#include <QTemporaryDir>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QFrame>
#include <QStyle>
#include <QSize>

#include <QAction>
#include <QCloseEvent>
#include <algorithm>
#include <QApplication>
#include <QDockWidget>
#include <QScrollArea>
#include <QProcess>
#include <QProcessEnvironment>
#include <QProgressDialog>
#include <QEventLoop>
#include <QDateTime>
#include <QListWidget>
#include <QSet>
#include <QThread>
#include <QStandardPaths>
#include <QFileDialog>
#include <QDialog>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QGroupBox>
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
#include "vortia/solver/openfoam/MassBalance.hpp"

namespace vortia {

namespace {

CaseSetup makeCavity() {
    CaseSetup s;
    s.domain = BoxDomain::fromExtents({0, 0, 0}, {0.1, 0.1, 0.01}).value();
    // A useful default for inspecting flow gradients on modern laptops.
    // Users can refine or coarsen these values from the Mesh properties.
    s.mesh.cells = {48, 48, 1};
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
    connect(jobController_, &JobController::jobFinished, console_, &ConsolePanel::flushOutput);
    connect(jobController_, &JobController::jobFinished, this, &MainWindow::onJobFinished);
    connect(jobController_, &JobController::assessmentUpdated,this,[this](const QString& report){
        showFlowAssessment(QJsonDocument::fromJson(report.toUtf8()).object());
    });
    connect(jobController_, &JobController::resultsReady, this, &MainWindow::onResultsReady);
    connect(jobController_, &JobController::progressUpdated, this,
            [this](double time, double coMax, const QString& residuals) {
                if(!flowConfig_.isEmpty()) {
                    setProjectState(tr("Calculating · iteration %1 · numerical checks active").arg(time,0,'g',9));return;
                }
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
    addProjectTab(tr("Untitled %1").arg(++untitledCount_), setup_);
    if(workspaceTabs_->currentWidget()==sketchEditor_)
        if(auto* dock=findChild<QDockWidget*>("vortia-dock-properties"))dock->hide();
    setProjectState(tr("New project: draw a closed fluid domain, then configure its mesh and flow."));
}

MainWindow::~MainWindow() {
    // Stop/join the worker before documents release their temporary case dirs.
    delete jobController_;
}

void MainWindow::buildMenusAndToolbar() {
    // Native macOS menu actions remain the source of truth. All ribbon
    // buttons use the SAME QAction instances (shortcuts, enabled state,
    // progress and accessibility stay synchronized).
    auto* fileMenu = menuBar()->addMenu(tr("File"));
    auto* newProject = fileMenu->addAction(tr("New Project"), this, &MainWindow::onNewProject, QKeySequence::New);
    auto* openProject = fileMenu->addAction(tr("Open Project…"), this, &MainWindow::onOpenProject, QKeySequence::Open);
    auto* openFoam = fileMenu->addAction(tr("Open OpenFOAM Case…"), this, &MainWindow::onOpenExistingCase);
    auto* saveProject = fileMenu->addAction(tr("Save Project"), this, &MainWindow::onSaveProject, QKeySequence::Save);
    newProjectAction_ = newProject;
    openProjectAction_ = openProject;
    openCaseAction_ = openFoam;
    fileMenu->addSeparator();
    auto* importStl = fileMenu->addAction(tr("Import STL…"), this, &MainWindow::onImportStl);
    auto* importFluent = fileMenu->addAction(tr("Import ANSYS Fluent…"), this, &MainWindow::onImportFluent);
    auto* openResults = fileMenu->addAction(tr("Open CFD results…"), this, &MainWindow::onOpenResults);
    auto* flowSetup = fileMenu->addAction(tr("Flow configuration…"), this, [this]() { configureFlow(1); });
    flowSetup->setMenuRole(QAction::NoRole);
    flowSetup->setObjectName(QStringLiteral("vortia-flow-setup-action"));
    flowSetup->setIcon(QIcon(QStringLiteral(":/vortia/icons/solver.svg")));
    auto* generateMesh = new QAction(QIcon(QStringLiteral(":/vortia/icons/mesh.svg")),tr("Generate mesh from sketch"),this);
    generateMesh->setObjectName(QStringLiteral("vortia-generate-flow-mesh"));
    connect(generateMesh,&QAction::triggered,this,&MainWindow::onGenerateFlowMesh);
    openResults->setIcon(QIcon(QStringLiteral(":/vortia/icons/view.svg")));
    fileMenu->addSeparator();
    fileMenu->addAction(tr("Exit"), this, &QWidget::close, QKeySequence::Quit);

    auto* simMenu = menuBar()->addMenu(tr("Simulation"));
    fileMenu->removeAction(flowSetup);simMenu->addAction(flowSetup);simMenu->addSeparator();
    auto* detectBackend = simMenu->addAction(tr("Detect OpenFOAM"), this, &MainWindow::onDetectOpenFoam);
    runAction_ = simMenu->addAction(tr("Run Simulation"), this, &MainWindow::onRun, QKeySequence(tr("Ctrl+R")));
    cancelAction_ = simMenu->addAction(tr("Cancel Simulation"), this, &MainWindow::onCancel);
    cancelAction_->setEnabled(false);

    auto* preferences = new QAction(tr("Preferences…"), this);
    preferences->setMenuRole(QAction::PreferencesRole);
    connect(preferences, &QAction::triggered, this, &MainWindow::onPreferences);
    auto* toolsMenu = menuBar()->addMenu(tr("Tools"));
    simMenu->removeAction(detectBackend);
    toolsMenu->addAction(preferences);toolsMenu->addAction(detectBackend);
    auto* searchTool=new QAction(tr("Find a tool…"),this);searchTool->setObjectName("vortia-find-tool");
    searchTool->setShortcut(QKeySequence("Ctrl+K"));searchTool->setIcon(QIcon(":/vortia/icons/view.svg"));
    toolsMenu->addAction(searchTool);connect(searchTool,&QAction::triggered,this,&MainWindow::onFindTool);
    preferences->setObjectName("vortia-preferences-action");preferences->setShortcut(QKeySequence::Preferences);
    preferences->setIcon(QIcon(":/vortia/icons/settings.svg"));

    auto* viewMenu = menuBar()->addMenu(tr("View"));
    viewMenu->setObjectName(QStringLiteral("vortia-view-menu"));
    themeAction_ = viewMenu->addAction(tr("Toggle Dark/Light Theme"), this, &MainWindow::onToggleTheme);

    auto* helpMenu = menuBar()->addMenu(tr("Help"));
    helpMenu->addAction(tr("About Vortia"), this, &MainWindow::onAbout);

    // Use Qt style icons so that no third-party icons/fonts or platform-specific
    // assets need bundling, and provide real actions rather than decorative tools.
    const auto icon = [this](QStyle::StandardPixmap type) { return style()->standardIcon(type); };
    // Original stroke-based Vortia icons; retain Qt style fallbacks when SVG
    // support is unavailable in a downstream Qt packaging environment.
    const auto toolIcon = [this](const QString& id, QStyle::StandardPixmap fallback) {
        const QIcon vector(QStringLiteral(":/vortia/icons/") + id + QStringLiteral(".svg"));
        return vector.isNull() ? style()->standardIcon(fallback) : vector;
    };
    newProject->setIcon(toolIcon(QStringLiteral("new"), QStyle::SP_FileIcon));
    openProject->setIcon(toolIcon(QStringLiteral("open"), QStyle::SP_DialogOpenButton));
    openFoam->setIcon(toolIcon(QStringLiteral("fluent"), QStyle::SP_DirOpenIcon));
    saveProject->setIcon(toolIcon(QStringLiteral("save"), QStyle::SP_DialogSaveButton));
    importStl->setIcon(toolIcon(QStringLiteral("import"), QStyle::SP_FileDialogDetailedView));
    importFluent->setIcon(toolIcon(QStringLiteral("fluent"), QStyle::SP_DriveHDIcon));
    detectBackend->setIcon(toolIcon(QStringLiteral("solver"), QStyle::SP_ComputerIcon));
    runAction_->setIcon(toolIcon(QStringLiteral("run"), QStyle::SP_MediaPlay));
    cancelAction_->setIcon(toolIcon(QStringLiteral("stop"), QStyle::SP_MediaStop));
    themeAction_->setIcon(toolIcon(QStringLiteral("settings"), QStyle::SP_DesktopIcon));


    auto* meshEditor = new QAction(toolIcon(QStringLiteral("mesh"), QStyle::SP_FileDialogDetailedView), tr("Mesh"), this);
    auto* materialsEditor = new QAction(toolIcon(QStringLiteral("material"), QStyle::SP_DriveHDIcon), tr("Materials"), this);
    auto* boundaryEditor = new QAction(toolIcon(QStringLiteral("boundary"), QStyle::SP_DirLinkIcon), tr("Boundaries"), this);
    auto* solverEditor = new QAction(toolIcon(QStringLiteral("solver"), QStyle::SP_ComputerIcon), tr("Solver"), this);
    auto* sketchEditor = new QAction(toolIcon(QStringLiteral("sketch"), QStyle::SP_FileDialogDetailedView), tr("2D Sketch…"), this);
    sketchEditor->setToolTip(tr("Draw a dimensioned polyline with split segments and exact mm lengths."));
    connect(sketchEditor, &QAction::triggered, this, [this]() {
        if (workspaceTabs_ && sketchEditor_)
            workspaceTabs_->setCurrentWidget(sketchEditor_);
    });

    connect(meshEditor, &QAction::triggered, this,
            [this]() { onNodeActivated(ProjectExplorerPanel::MeshNode); });
    connect(materialsEditor, &QAction::triggered, this,
            [this]() { onNodeActivated(ProjectExplorerPanel::MaterialsNode); });
    connect(boundaryEditor, &QAction::triggered, this,
            [this]() { onNodeActivated(ProjectExplorerPanel::BoundaryConditionsNode); });
    connect(solverEditor, &QAction::triggered, this,
            [this]() { onNodeActivated(ProjectExplorerPanel::SolverNode); });

    auto* viewXY = new QAction(icon(QStyle::SP_ArrowUp), tr("XY"), this);
    auto* viewXZ = new QAction(icon(QStyle::SP_ArrowRight), tr("XZ"), this);
    auto* viewYZ = new QAction(icon(QStyle::SP_ArrowLeft), tr("YZ"), this);
    auto* viewIso = new QAction(icon(QStyle::SP_FileDialogDetailedView), tr("3D"), this);
    auto* viewReset = new QAction(icon(QStyle::SP_BrowserReload), tr("Fit View"), this);
    connect(viewXY, &QAction::triggered, this, [this]() { viewport_->setViewXY(); });
    connect(viewXZ, &QAction::triggered, this, [this]() { viewport_->setViewXZ(); });
    connect(viewYZ, &QAction::triggered, this, [this]() { viewport_->setViewYZ(); });
    connect(viewIso, &QAction::triggered, this, [this]() { viewport_->setViewIsometric(); });
    connect(viewReset, &QAction::triggered, this, [this]() { viewport_->resetCamera(); });

    auto* toolbar = addToolBar(tr("Engineering tools"));
    toolbar->setObjectName(QStringLiteral("vortia-main-toolbar"));
    toolbar->setMovable(false);
    toolbar->setFloatable(false);
    auto* tabs = new QTabWidget(toolbar);
    tabs->setObjectName(QStringLiteral("vortia-ribbon"));
    tabs->setDocumentMode(true);
    tabs->setUsesScrollButtons(true);
    tabs->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    tabs->setMinimumHeight(126);

    auto newPage = [this, tabs](const QString& name) {
        auto* page = new QWidget(tabs);
        auto* layout = new QHBoxLayout(page);
        layout->setContentsMargins(12, 6, 12, 3);
        layout->setSpacing(11);
        layout->setAlignment(Qt::AlignLeft);
        tabs->addTab(page, name);
        return layout;
    };
    auto addGroup = [this](QHBoxLayout* parent, const QString& title,
                           std::initializer_list<QAction*> actions) {
        auto* group = new QWidget(this);
        auto* vbox = new QVBoxLayout(group);
        vbox->setContentsMargins(0, 0, 0, 0);
        vbox->setSpacing(2);
        auto* buttons = new QHBoxLayout();
        buttons->setSpacing(2);
        for (auto* action : actions) {
            auto* button = new QToolButton(group);
            button->setObjectName(QStringLiteral("vortia-ribbon-button"));
            if(action->menu())button->setMenu(action->menu());
            button->setDefaultAction(action);
            if(action->menu())button->setPopupMode(QToolButton::InstantPopup);
            button->setToolButtonStyle(Qt::ToolButtonTextUnderIcon);
            button->setIconSize(QSize(28, 28));
            button->setMinimumSize(QSize(78, 69));
            button->setMaximumWidth(144);
            button->setAccessibleName(action->text());
            buttons->addWidget(button);
        }
        vbox->addLayout(buttons);
        auto* caption = new QLabel(title, group);
        caption->setObjectName(QStringLiteral("vortia-ribbon-group"));
        caption->setAlignment(Qt::AlignCenter);
        vbox->addWidget(caption);
        parent->addWidget(group);
        auto* separator = new QFrame(this);
        separator->setFrameShape(QFrame::VLine);
        separator->setObjectName(QStringLiteral("vortia-ribbon-separator"));
        parent->addWidget(separator);
    };

    newProject->setIconText(tr("New"));openProject->setIconText(tr("Open"));saveProject->setIconText(tr("Save"));
    importFluent->setIconText(tr("Fluent case"));sketchEditor->setIconText(tr("Edit sketch"));
    generateMesh->setIconText(tr("Generate mesh"));meshEditor->setIconText(tr("Mesh controls"));
    materialsEditor->setIconText(tr("Models"));boundaryEditor->setIconText(tr("Boundaries"));
    solverEditor->setIconText(tr("Run controls"));runAction_->setIconText(tr("Run"));cancelAction_->setIconText(tr("Stop"));
    openResults->setIconText(tr("Open results"));searchTool->setIconText(tr("Find a tool"));
    auto* homePage = newPage(tr("Home"));
    addGroup(homePage, tr("Project"), {newProject, openProject, saveProject});
    auto* importMenu=new QMenu(this);importMenu->addActions({openFoam,importStl,importFluent});
    auto* importAction=new QAction(toolIcon("import",QStyle::SP_DialogOpenButton),tr("Import…"),this);
    importAction->setObjectName("vortia-import-menu");importAction->setMenu(importMenu);
    addGroup(homePage,tr("Files"),{importAction});addGroup(homePage,tr("Application"),{preferences,searchTool});

    auto* geometryPage=newPage(tr("Geometry"));
    auto* sketchFiles=new QMenu(this);
    auto* loadSketch=sketchFiles->addAction(tr("Import sketch…"));
    auto* saveSketch=sketchFiles->addAction(tr("Export sketch…"));sketchFiles->addSeparator();
    auto* planar=sketchFiles->addAction(tr("Export planar block mesh…"));
    auto* wedge=sketchFiles->addAction(tr("Export wedge block mesh…"));
    connect(loadSketch,&QAction::triggered,this,[this](){if(!projectSwitchLocked()&&!useExistingCase_)sketchEditor_->loadSketch();});
    connect(saveSketch,&QAction::triggered,this,[this](){sketchEditor_->saveSketch();});
    connect(planar,&QAction::triggered,this,[this](){sketchEditor_->exportNozzleMesh();});
    connect(wedge,&QAction::triggered,this,[this](){sketchEditor_->exportWedgeMesh();});
    auto* sketchFilesAction=new QAction(toolIcon("save",QStyle::SP_DialogSaveButton),tr("Sketch files…"),this);
    sketchFilesAction->setMenu(sketchFiles);sketchFilesAction->setToolTip(tr("Import/export CAD sketches or standalone block meshes. Generate mesh builds the CFD domain."));
    addGroup(geometryPage,tr("Sketch"),{sketchEditor,sketchFilesAction});
    addGroup(geometryPage,tr("Mesh"),{meshEditor,generateMesh});

    auto* physicsPage=newPage(tr("Physics"));
    addGroup(physicsPage,tr("Physical setup"),{materialsEditor,boundaryEditor});

    auto* simPage=newPage(tr("Calculation"));
    addGroup(simPage,tr("Controls"),{solverEditor});
    addGroup(simPage,tr("Execution"),{runAction_,cancelAction_});
    auto* viewLatest=new QAction(toolIcon("view",QStyle::SP_DialogApplyButton),tr("View latest result"),this);
    viewLatest->setIconText(tr("View latest"));
    viewLatest->setObjectName("vortia-view-latest-result");
    connect(viewLatest,&QAction::triggered,this,&MainWindow::onViewLatestResult);

    auto* resultsPage = newPage(tr("Results"));
    auto* profileAction=new QAction(QIcon(QStringLiteral(":/vortia/icons/chart.svg")),tr("Line profile…"),this);
    connect(profileAction,&QAction::triggered,this,[this](){showLineProfile(this,viewport_,fieldCombo_->currentData().toString());});
    auto* edgeAction=new QAction(QIcon(QStringLiteral(":/vortia/icons/mesh.svg")),tr("Show cell edges"),this);
    edgeAction->setIconText(tr("Cell edges"));
    edgeAction->setCheckable(true);
    connect(edgeAction,&QAction::toggled,this,[this](bool enabled){viewport_->showCellEdges(enabled);});

    auto* fields = new QWidget(this);
    auto* fv = new QVBoxLayout(fields);
    fv->setContentsMargins(0, 0, 0, 0);
    fv->setSpacing(7);
    auto* fieldTitle = new QLabel(tr("DISPLAY FIELD"), fields);
    fieldTitle->setObjectName(QStringLiteral("vortia-ribbon-group"));
    fv->addWidget(fieldTitle);
    fieldCombo_ = new QComboBox(fields);
    fieldCombo_->setObjectName(QStringLiteral("vortia-field-selector"));
    fieldCombo_->setMinimumWidth(160);fieldCombo_->setMaximumWidth(230);
    fieldCombo_->setSizeAdjustPolicy(QComboBox::AdjustToMinimumContentsLengthWithIcon);fieldCombo_->setMinimumContentsLength(14);
    for(const auto& field:std::vector<std::pair<QString,QString>>{
        {tr("Pressure · p"),"p"},{tr("Speed · |U|"),"|U|"},{tr("Velocity X"),"Ux"},
        {tr("Velocity Y"),"Uy"},{tr("Velocity Z"),"Uz"},{tr("Temperature · T"),"T"},
        {tr("Mach number"),"Mach"},{tr("Turbulent viscosity · nut"),"nut"}})fieldCombo_->addItem(field.first,field.second);
    fieldCombo_->setToolTip(tr("Choose a field from the current CFD result"));
    fv->addWidget(fieldCombo_);
    connect(fieldCombo_, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &MainWindow::onFieldChanged);
    resultsPage->addWidget(fields);
    fieldCombo_->setEnabled(false);
    auto* resultMenu=menuBar()->addMenu(tr("Results"));resultMenu->setObjectName("vortia-results-menu");
    fileMenu->removeAction(openResults);resultMenu->addActions({viewLatest,openResults});resultMenu->addSeparator();
    const auto menuAction=[&](QMenu* menu,const QString& name,const QString& glyph){menu->setTitle(name);menu->setIcon(toolIcon(glyph,QStyle::SP_FileDialogDetailedView));auto* action=menu->menuAction();return action;};
    auto* dataMenu=new QMenu(this);dataMenu->addActions({viewLatest,openResults});
    addGroup(resultsPage,tr("Data"),{menuAction(dataMenu,tr("Result files"),"open")});
    auto* section=new QAction(toolIcon("slice",QStyle::SP_FileDialogDetailedView),tr("Plane section…"),this);section->setObjectName("vortia-plane-section-action");
    auto* full=new QAction(toolIcon("surface",QStyle::SP_FileDialogDetailedView),tr("Show full domain"),this);full->setObjectName("vortia-show-full-domain");
    connect(section,&QAction::triggered,this,[this](){showPlaneSection(this,viewport_);});
    connect(full,&QAction::triggered,this,[this](){viewport_->clearSlicePlane();viewport_->resetCamera();});
    auto* point=new QAction(toolIcon("probe",QStyle::SP_FileDialogDetailedView),tr("Probe a point…"),this);point->setObjectName("vortia-probe-point-action");
    connect(point,&QAction::triggered,this,[this](){showPointProbe(this,viewport_,fieldCombo_->currentData().toString());});
    auto* sectionMenu=new QMenu(this);sectionMenu->addActions({section,full});
    auto* queryMenu=new QMenu(this);queryMenu->addActions({point,profileAction});
    auto* sections=menuAction(sectionMenu,tr("Sections"),"slice");auto* queries=menuAction(queryMenu,tr("Sample"),"probe");
    fieldActions_={profileAction,point,section,full,sections,queries};
    for(auto* action:fieldActions_)action->setEnabled(false);
    addGroup(resultsPage,tr("Analyze"),{sections,queries});resultMenu->addMenu(sectionMenu);resultMenu->addMenu(queryMenu);
    auto* displayMenu=new QMenu(this);auto* representation=new QActionGroup(this);
    auto* surface=displayMenu->addAction(toolIcon("surface",QStyle::SP_FileIcon),tr("Surface"));surface->setCheckable(true);surface->setChecked(true);representation->addAction(surface);
    auto* wire=displayMenu->addAction(toolIcon("wireframe",QStyle::SP_FileIcon),tr("Wireframe"));wire->setCheckable(true);representation->addAction(wire);
    displayMenu->addSeparator();displayMenu->addAction(edgeAction);
    connect(surface,&QAction::triggered,this,[this](){viewport_->setWireframe(false);});connect(wire,&QAction::triggered,this,[this](){viewport_->setWireframe(true);});
    auto* cameraMenu=new QMenu(this);cameraMenu->setTitle(tr("Camera"));cameraMenu->setIcon(toolIcon("camera",QStyle::SP_DesktopIcon));
    viewReset->setIcon(toolIcon("fit",QStyle::SP_DialogResetButton));cameraMenu->addActions({viewReset,viewXY,viewXZ,viewYZ,viewIso});cameraMenu->addSeparator();
    auto* parallel=cameraMenu->addAction(tr("Parallel projection"));parallel->setCheckable(true);
    connect(parallel,&QAction::triggered,this,[this](bool checked){viewport_->setParallelProjection(checked);});
    connect(cameraMenu,&QMenu::aboutToShow,this,[this,parallel](){parallel->setChecked(viewport_->parallelProjection());});
    auto* navigation=cameraMenu->addMenu(tr("Mouse navigation"));auto* navigationModes=new QActionGroup(this);
    const std::array<std::tuple<QString,QString,VtkViewport::NavigationMode>,3> modes{{{tr("Orbit"),"orbit",VtkViewport::NavigationMode::Orbit},{tr("Pan"),"pan",VtkViewport::NavigationMode::Pan},{tr("Zoom"),"zoom",VtkViewport::NavigationMode::Zoom}}};
    for(const auto& [label,glyph,mode]:modes){auto* action=navigation->addAction(toolIcon(glyph,QStyle::SP_ArrowUp),label);action->setCheckable(true);action->setChecked(mode==VtkViewport::NavigationMode::Orbit);navigationModes->addAction(action);connect(action,&QAction::triggered,this,[this,mode](){viewport_->setNavigationMode(mode);});}
    addGroup(resultsPage,tr("View"),{menuAction(displayMenu,tr("Display"),"surface"),cameraMenu->menuAction()});viewMenu->addMenu(cameraMenu);viewMenu->addMenu(displayMenu);
    auto* image=new QAction(toolIcon("export-image",QStyle::SP_DialogSaveButton),tr("Export view PNG…"),this);image->setIconText(tr("Save image"));image->setObjectName("vortia-export-view-image");
    connect(image,&QAction::triggered,this,[this](){exportViewportImage(this,viewport_);});addGroup(resultsPage,tr("Export"),{image});resultMenu->addAction(image);

    toolbar->addWidget(tabs);
    statusLabel_ = new QLabel(this);
    statusLabel_->setObjectName(QStringLiteral("vortia-status-text"));
    statusBar()->addWidget(statusLabel_, 1);
}

void MainWindow::buildDocks() {
    setDockOptions(QMainWindow::AnimatedDocks | QMainWindow::AllowTabbedDocks
                   | QMainWindow::AllowNestedDocks);
    auto* central = new QWidget(this);
    auto* centralLayout = new QVBoxLayout(central);
    centralLayout->setContentsMargins(0, 0, 0, 0);
    centralLayout->setSpacing(0);
    projectTabs_ = new QTabBar(central);
    projectTabs_->setObjectName(QStringLiteral("vortia-project-tabs"));
    projectTabs_->setMovable(false);
    projectTabs_->setTabsClosable(true);
    projectTabs_->setDocumentMode(true);
    projectTabs_->setExpanding(false);
    projectTabs_->setUsesScrollButtons(true);
    centralLayout->addWidget(projectTabs_);
    nozzleNotice_ = new QLabel(central);
    nozzleNotice_->setObjectName(QStringLiteral("vortia-nozzle-validation-notice"));
    nozzleNotice_->setWordWrap(true);
    nozzleNotice_->setMargin(8);
    nozzleNotice_->setStyleSheet(QStringLiteral("QLabel { background: #584719; color: #fff0bd; }"));
    nozzleNotice_->hide();
    centralLayout->addWidget(nozzleNotice_);
    workspaceTabs_ = new QTabWidget(central);
    workspaceTabs_->setDocumentMode(true);
    workspaceTabs_->setObjectName(QStringLiteral("vortia-editor-tabs"));
    viewport_ = new VtkViewport(workspaceTabs_);
    connect(viewport_,&VtkViewport::fieldDataChanged,this,[this](){refreshFieldTools();});
    workspaceTabs_->addTab(viewport_, tr("3D CFD"));
    sketchEditor_ = new SketchEditorDialog(workspaceTabs_);
    sketchEditor_->setWindowFlags(Qt::Widget);sketchEditor_->setEmbedded(true);
    workspaceTabs_->addTab(sketchEditor_, tr("2D Sketch"));
    connect(sketchEditor_, &SketchEditorDialog::closeRequested, this,
            [this]() { workspaceTabs_->setCurrentWidget(viewport_); });
    connect(sketchEditor_, &SketchEditorDialog::sketchChanged, this, [this]() {
        dirty_ = true;
        flowConfig_.remove("preparedHash");
        storeCurrentTab();
        refreshProjectTabTitle();
        updateNozzleNotice();
    });
    centralLayout->addWidget(workspaceTabs_, 1);
    setCentralWidget(central);
    connect(projectTabs_, &QTabBar::currentChanged,
            this, &MainWindow::activateProjectTab);
    connect(projectTabs_, &QTabBar::tabCloseRequested,
            this, &MainWindow::closeProjectTab);

    auto* explorerDock = new QDockWidget(tr("Project"), this);
    explorerDock->setAccessibleName(tr("Project Explorer"));
    explorerDock->setObjectName(QStringLiteral("vortia-dock-project"));
    explorerDock->setAllowedAreas(Qt::LeftDockWidgetArea | Qt::RightDockWidgetArea);
    explorer_ = new ProjectExplorerPanel(this);
    explorerDock->setWidget(explorer_);
    addDockWidget(Qt::LeftDockWidgetArea, explorerDock);

    auto* propsDock = new QDockWidget(tr("Properties"), this);
    propsDock->setObjectName(QStringLiteral("vortia-dock-properties"));
    propsDock->setAllowedAreas(Qt::LeftDockWidgetArea | Qt::RightDockWidgetArea);
    properties_ = new PropertiesPanel(this);
    auto* propertiesContainer=new QWidget(propsDock);auto* propertiesLayout=new QVBoxLayout(propertiesContainer);
    propertiesLayout->setContentsMargins(8,8,8,8);
    flowAssessment_=new QLabel(propertiesContainer);flowAssessment_->setObjectName("vortia-flow-assessment");
    flowAssessment_->setWordWrap(true);flowAssessment_->setTextFormat(Qt::RichText);
    flowAssessment_->setAlignment(Qt::AlignLeft|Qt::AlignTop);
    flowAssessment_->setStyleSheet("QLabel { border: 1px solid palette(mid); border-radius: 12px; padding: 14px; }");
    flowAssessment_->setMinimumWidth(250);flowAssessment_->hide();
    propertiesTabs_=new QTabWidget(propertiesContainer);
    propertiesTabs_->addTab(properties_,tr("Properties"));
    auto* assessmentScroll=new QScrollArea(propertiesTabs_);assessmentScroll->setWidgetResizable(true);
    assessmentScroll->setFrameShape(QFrame::NoFrame);assessmentScroll->setWidget(flowAssessment_);
    propertiesTabs_->addTab(assessmentScroll,tr("Run assessment"));propertiesTabs_->setTabVisible(1,false);
    propertiesLayout->addWidget(propertiesTabs_,1);
    propsDock->setWidget(propertiesContainer);
    addDockWidget(Qt::RightDockWidgetArea, propsDock);
    connect(workspaceTabs_,&QTabWidget::currentChanged,this,[this,propsDock](int){
        if(workspaceTabs_->currentWidget()==sketchEditor_&&!projectSwitchLocked())propsDock->hide();
    });

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
    preferences_=ApplicationPreferences::load(*settings_);applyPreferences();
    if(!preferences_.rememberLayout)return;
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
    if(preferences_.rememberLayout) {
        settings_->setValue(QStringLiteral("window/geometry"), saveGeometry());
        settings_->setValue(QStringLiteral("window/state"), saveState());
    }
    settings_->sync();
}

void MainWindow::updateViewport() {
    viewport_->showBoxDomain(useExistingCase_ || !flowConfig_.isEmpty() ? nullptr : &setup_.domain);
    viewport_->setDarkTheme(ThemeManager::fromName(
        settings_->value(QStringLiteral("theme"), QStringLiteral("dark")).toString()) ==
        ThemeManager::Theme::Dark);
}

void MainWindow::storeCurrentTab() {
    if (activeProjectTab_ < 0 ||
        static_cast<std::size_t>(activeProjectTab_) >= documents_.size())
        return;
    auto& tab = documents_[static_cast<std::size_t>(activeProjectTab_)];
    tab.root = projectRoot_;
    tab.externalCase = existingCaseDir_;
    tab.existingCase = useExistingCase_;
    tab.setup = setup_;
    if (sketchEditor_) {
        tab.sketch = sketchEditor_->sketch();
        tab.nozzleDimensions = sketchEditor_->nozzleDimensions();
    }
    tab.modified = dirty_;
    tab.resultVtm = lastVtmPath_;
    tab.flowConfig = flowConfig_;
}

void MainWindow::refreshProjectTabTitle() {
    if (activeProjectTab_ < 0 ||
        static_cast<std::size_t>(activeProjectTab_) >= documents_.size())
        return;
    auto& tab = documents_[static_cast<std::size_t>(activeProjectTab_)];
    const QString name = projectRoot_.empty() ? tab.name :
        QString::fromStdString(projectRoot_.filename().string());
    tab.name = name;
    projectTabs_->setTabText(activeProjectTab_, name + (dirty_ ? QStringLiteral(" *") : QString()));
    const auto& storedPath = useExistingCase_ ? existingCaseDir_ : projectRoot_;
    projectTabs_->setTabToolTip(activeProjectTab_, storedPath.empty() ? tr("Unsaved project")
                             : QString::fromStdString(storedPath.string()));
}

void MainWindow::addProjectTab(const QString& name, CaseSetup setup,
                              const std::filesystem::path& root,
                              const std::filesystem::path& caseDir) {
    if (projectSwitchLocked()) return;
    storeCurrentTab();
    ProjectDocument doc;
    doc.name = name;
    doc.setup = std::move(setup);
    doc.root = root;
    doc.externalCase = caseDir;
    doc.existingCase = !caseDir.empty();
    const int index = static_cast<int>(documents_.size());
    documents_.push_back(std::move(doc));
    {
        QSignalBlocker block(projectTabs_);
        projectTabs_->addTab(name);
    }
    projectTabs_->setCurrentIndex(index);
    if (activeProjectTab_ != index)
        activateProjectTab(index);
}

void MainWindow::activateProjectTab(int index) {
    if (index < 0 || static_cast<std::size_t>(index) >= documents_.size()
        || index == activeProjectTab_)
        return;
    if (projectSwitchLocked()) {
        // A solver run always belongs to the active case. Do not allow the
        // result/console to be attributed to a different document.
        QSignalBlocker block(projectTabs_);
        projectTabs_->setCurrentIndex(activeProjectTab_);
        return;
    }
    storeCurrentTab();
    activeProjectTab_ = index;
    const auto& tab = documents_[static_cast<std::size_t>(index)];
    projectRoot_ = tab.root;
    existingCaseDir_ = tab.externalCase;
    useExistingCase_ = tab.existingCase;
    setup_ = tab.setup;
    flowConfig_ = tab.flowConfig;
    dirty_ = tab.modified;
    lastVtmPath_ = tab.resultVtm;
    updateNozzleNotice();
    properties_->setSetup(setup_);
    // An external case owns its physics dictionaries. Never show the cavity
    // editor as though it configured that case, or edit an uncoupled sketch.
    properties_->setEnabled(!useExistingCase_ && flowConfig_.isEmpty());
    sketchEditor_->setReadOnly(useExistingCase_);
    if (sketchEditor_) {
        sketchEditor_->setNozzleDimensions(tab.nozzleDimensions);
        sketchEditor_->setSketch(tab.sketch);
    }
    viewport_->clearScene();
    updateViewport();
    if (!lastVtmPath_.isEmpty() && QFileInfo::exists(lastVtmPath_)) {
        try {
            viewport_->loadVtmFile(lastVtmPath_.toStdString(),
                                   fieldCombo_->currentData().toString().toStdString());
        } catch (...) {
            // A stale VTM on disk must never crash project switching.
            lastVtmPath_.clear();
        }
    }
    refreshProjectTabTitle();
    setProjectState(tr("Active project: %1").arg(tab.name));
}

void MainWindow::closeProjectTab(int index) {
    if (index < 0 || static_cast<std::size_t>(index) >= documents_.size()
        || projectSwitchLocked())
        return;
    storeCurrentTab();
    const auto& tab = documents_[static_cast<std::size_t>(index)];
    if (tab.modified && QMessageBox::question(
            this, tr("Unsaved project"),
            tr("Discard unsaved changes in %1?").arg(tab.name),
            QMessageBox::Yes | QMessageBox::No, QMessageBox::No) != QMessageBox::Yes)
        return;
    const int oldActive = activeProjectTab_;
    {
        QSignalBlocker block(projectTabs_);
        documents_.erase(documents_.begin() + index);
        projectTabs_->removeTab(index);
    }
    activeProjectTab_ = -1;
    if (documents_.empty()) {
        addProjectTab(tr("Untitled %1").arg(++untitledCount_), makeCavity());
    } else {
        const int next = index == oldActive
            ? std::min(index, static_cast<int>(documents_.size()) - 1)
            : oldActive - (index < oldActive ? 1 : 0);
        {
            QSignalBlocker block(projectTabs_);
            projectTabs_->setCurrentIndex(next);
        }
        activateProjectTab(next);
    }
}

void MainWindow::onNewProject() {
    if (projectSwitchLocked()) return;
    addProjectTab(tr("Untitled %1").arg(++untitledCount_), makeCavity());
    setProjectState(tr("New project: draw a closed fluid domain, then configure its mesh and flow."));
}

void MainWindow::onOpenProject() {
    if (projectSwitchLocked()) return;
    const QString dir = QFileDialog::getExistingDirectory(this, tr("Open Vortia Project"));
    if (dir.isEmpty())
        return;
    openProjectDirectory(dir);
}

bool MainWindow::openProjectDirectory(const QString& dir) {
    if (projectSwitchLocked()) return false;
    // Reopening a directory must focus its document, never create competing
    // in-memory copies that can overwrite each other's saved sketch.
    const QString canonical = QFileInfo(dir).canonicalFilePath();
    for (std::size_t i = 0; i < documents_.size(); ++i) {
        const auto& doc = documents_[i];
        const auto path = doc.root.empty() ? doc.externalCase : doc.root;
        if (!canonical.isEmpty() && !path.empty()
            && QFileInfo(QString::fromStdString(path.string())).canonicalFilePath() == canonical) {
            projectTabs_->setCurrentIndex(static_cast<int>(i));
            return true;
        }
    }
    // Native folder pickers may append a slash; filesystem::path::filename()
    // would then be empty and erase the project tab label on activation.
    const QString normalizedDir = canonical.isEmpty() ? dir : canonical;
    auto project = Project::open(normalizedDir.toStdString());
    if (!project) {
        // Not a Vortia project — maybe it's an OpenFOAM case directory.
        if (!std::filesystem::exists(std::filesystem::path(dir.toStdString()) / std::string(Project::kManifestFileName))
            && std::filesystem::exists(dir.toStdString() + "/system/controlDict")) {
            SketchEditorDialog staged;
            const QString sketchPath=QDir(normalizedDir).filePath(QStringLiteral("geometry/working-sketch.vsketch.json"));
            if (QFileInfo::exists(sketchPath) && !staged.readSketchFile(sketchPath)) {
                QMessageBox::warning(this,tr("Sketch load failed"),tr("This project contains an invalid 2D sketch file."));
                return false;
            }
            addProjectTab(QFileInfo(normalizedDir).fileName(), makeCavity(),
                          {}, std::filesystem::path(normalizedDir.toStdString()));
            sketchEditor_->setSketch(staged.sketch());
            storeCurrentTab();
            const auto result=currentVtmFile(normalizedDir);
            if (!result.isEmpty()) onResultsReady(result);
            setProjectState(tr("Opened existing OpenFOAM case: %1").arg(dir));
            console_->appendSolverOutput(tr("Opened existing OpenFOAM case: %1\n").arg(dir));
            return true;
        }
        QMessageBox::warning(this, tr("Open failed"), QString::fromStdString(project.error().describe()));
        return false;
    }
    CaseSetup openedSetup = makeCavity();
    if (!project->setupJson().empty()) {
        auto loaded = caseSetupFromJson(project->setupJson());
        if (loaded)
            openedSetup = loaded.value();
        else {
            QMessageBox::warning(this, tr("Open warning"),
                                 tr("Case setup could not be fully restored: %1")
                                     .arg(QString::fromStdString(loaded.error().describe())));
            return false;
        }
    }
    // Parse into a staging editor before changing any active document. A bad
    // saved profile must never become an empty sketch that Save overwrites.
    SketchEditorDialog staged;
    const auto sketchPath = project->rootDir() / "geometry" / "working-sketch.vsketch.json";
    if (std::filesystem::exists(sketchPath) &&
        !staged.readSketchFile(QString::fromStdString(sketchPath.string()))) {
        QMessageBox::warning(this, tr("Sketch load failed"),
                             tr("This project contains an invalid 2D sketch file."));
        return false;
    }
    QJsonObject openedFlow;
    QFile flowFile(QString::fromStdString((project->rootDir()/"flow-setup.json").string()));
    if (flowFile.exists()) {
        if (!flowFile.open(QIODevice::ReadOnly)) return false;
        const auto flowDocument=QJsonDocument::fromJson(flowFile.readAll());
        if (!flowDocument.isObject() || flowDocument.object().isEmpty()) {
            QMessageBox::warning(this,tr("Open failed"),tr("The saved flow configuration is invalid.")); return false;
        }
        openedFlow=flowDocument.object();
        if(!openedFlow.contains("preparedEngine")&&!project->backendFingerprint().empty())
            openedFlow.insert("preparedEngine",QString::fromStdString(project->backendFingerprint()));
    }
    addProjectTab(QString::fromStdString(project->name()), openedSetup,
                  project->rootDir());
    flowConfig_=openedFlow;
    properties_->setEnabled(flowConfig_.isEmpty());
    updateViewport();
    updateNozzleNotice();
    const auto flowResult=currentVtmFile(flowConfig_.value("activeCase").toString());
    if (!flowConfig_.isEmpty() && !flowResult.isEmpty()) onResultsReady(flowResult);
    sketchEditor_->setNozzleDimensions(staged.nozzleDimensions());
    sketchEditor_->setSketch(staged.sketch());
    storeCurrentTab();
    setProjectState(tr("Opened project: %1").arg(QString::fromStdString(project->name())));
    return true;
}

void MainWindow::onSaveProject() {
    saveProject();
}

bool MainWindow::saveProject() {
    if (useExistingCase_) {
        QMessageBox::information(this,tr("Existing OpenFOAM case"),
            tr("This case stores its geometry and physics in its own folder. It is not saved as a cavity project. "
               "Open its editable source project to change geometry and generate a new mesh revision."));
        return false;
    }
    if (projectRoot_.empty()) {
        const QString dir = QFileDialog::getExistingDirectory(this, tr("Choose project location"));
        if (dir.isEmpty())
            return false;
        const QString name = QInputDialog::getText(this, tr("Project name"), tr("Name:"), QLineEdit::Normal,
                                                   QStringLiteral("proyecto-flujo"));
        if (name.isEmpty())
            return false;
        auto project = Project::create((dir.toStdString() + "/" + name.toStdString()), name.toStdString());
        if (!project) {
            QMessageBox::warning(this, tr("Save failed"), QString::fromStdString(project.error().describe()));
            return false;
        }
        projectRoot_ = project->rootDir();
    }

    auto project = Project::open(projectRoot_);
    if (!project) {
        QMessageBox::warning(this, tr("Save failed"), QString::fromStdString(project.error().describe()));
        return false;
    }
    project->setName(projectRoot_.filename().string());
    project->setSetupJson(caseSetupToJson(setup_));
    if (hasOfInstallation_)
        project->setBackendFingerprint(ofInstallation_.fingerprint());
    if (std::filesystem::exists(projectRoot_ / "case" / openfoam::CaseWriter::kMarkerFile))
        project->setCaseState(Project::CaseState::Configured);
    if (auto saved = project->save(); !saved) {
        QMessageBox::warning(this, tr("Save failed"), QString::fromStdString(saved.error().describe()));
        return false;
    }
    const auto sketchPath = projectRoot_ / "geometry" / "working-sketch.vsketch.json";
    if (!sketchEditor_->writeSketchFile(QString::fromStdString(sketchPath.string()))) {
        QMessageBox::warning(this, tr("Sketch save failed"),
                             tr("The project metadata was saved, but its 2D sketch could not be written."));
        dirty_ = true;
        refreshProjectTabTitle();
        storeCurrentTab();
        return false;
    }
    if (!flowConfig_.isEmpty()) {
        QSaveFile flowFile(QString::fromStdString((projectRoot_/"flow-setup.json").string()));
        const auto data=QJsonDocument(flowConfig_).toJson();
        if (!flowFile.open(QIODevice::WriteOnly) || flowFile.write(data)!=data.size() || !flowFile.commit()) {
            QMessageBox::warning(this,tr("Save failed"),tr("The flow configuration could not be saved.")); return false;
        }
    }
    dirty_ = false;
    refreshProjectTabTitle();
    storeCurrentTab();
    updateNozzleNotice();
    setProjectState(tr("Saved project: %1").arg(QString::fromStdString(project->name())));
    return true;
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
    if (projectSwitchLocked()) return;
    const QString dir = QFileDialog::getExistingDirectory(this, tr("Open OpenFOAM Case"));
    if (dir.isEmpty())
        return;
    const std::filesystem::path caseDir = dir.toStdString();
    if (!std::filesystem::exists(caseDir / "system" / "controlDict")) {
        QMessageBox::warning(this, tr("Not an OpenFOAM case"),
                             tr("The selected directory has no system/controlDict."));
        return;
    }
    openProjectDirectory(dir);
    setProjectState(tr("Opened existing case: %1").arg(dir));
    console_->appendSolverOutput(tr("Opened existing OpenFOAM case: %1\n").arg(dir));
}

void MainWindow::onNodeActivated(int nodeId) {
    if(auto* ribbon=findChild<QTabWidget*>("vortia-ribbon")) {
        const int stage=nodeId==ProjectExplorerPanel::GeometryNode||nodeId==ProjectExplorerPanel::MeshNode?1:
            nodeId==ProjectExplorerPanel::MaterialsNode||nodeId==ProjectExplorerPanel::BoundaryConditionsNode?2:
            nodeId==ProjectExplorerPanel::SolverNode?3:4;
        ribbon->setCurrentIndex(stage);
    }
    const bool sketchWorkflow=!useExistingCase_&&(!flowConfig_.isEmpty()||sketchEditor_->sketch().segmentCount()
        ||projectRoot_.empty()||!std::filesystem::exists(projectRoot_/"case/system/controlDict"));
    if (sketchWorkflow) {
        if (nodeId==ProjectExplorerPanel::MeshNode) { configureFlow(0);return; }
        if (nodeId==ProjectExplorerPanel::MaterialsNode) { configureFlow(1);return; }
        if (nodeId==ProjectExplorerPanel::BoundaryConditionsNode) { configureFlow(2);return; }
        if (nodeId==ProjectExplorerPanel::SolverNode) { configureFlow(3);return; }
    }
    if (!useExistingCase_) {
        properties_->showNode(nodeId);
        if(!sketchWorkflow&&nodeId!=ProjectExplorerPanel::GeometryNode)
            if(auto* dock=findChild<QDockWidget*>("vortia-dock-properties"))dock->show();
    }
    if (workspaceTabs_) {
        workspaceTabs_->setCurrentWidget(
            nodeId == ProjectExplorerPanel::GeometryNode ? static_cast<QWidget*>(sketchEditor_)
                                                          : static_cast<QWidget*>(viewport_));
    }
}

void MainWindow::onApplySetup(const CaseSetup& setup) {
    if (useExistingCase_ || !flowConfig_.isEmpty()) return;
    setup_ = setup;
    dirty_ = true;
    storeCurrentTab();
    refreshProjectTabTitle();
    updateViewport();
}

void MainWindow::onValidationReport(const QString& text) {
    console_->appendSolverOutput(text + "\n");
}

void MainWindow::configureFlow(int page) {
    if (projectSwitchLocked()) return;
    if (useExistingCase_) {
        QMessageBox::information(this,tr("Imported case"),tr("Open the editable source project to change geometry and flow settings. Existing imported case files are preserved."));
        return;
    }
    const auto edited=editFlowSetup(this,sketchEditor_->sketch(),flowConfig_,page,preferences_.flowDefaults());
    if (edited.isEmpty()) return;
    // Migrate a verified pre-resume fingerprint before changing runtime controls.
    // Physics/geometry edits still invalidate it after the source project is saved.
    if(flowConfig_.value("preparedHash").toString()==flowFingerprint(true))
        flowConfig_.insert("preparedHash",flowFingerprint());
    auto old=flowConfig_; old.remove("activeCase"); old.remove("preparedHash");old.remove("preparedEngine");
    if (old!=edited) {
        const auto active=flowConfig_.value("activeCase"),prepared=flowConfig_.value("preparedHash"),engine=flowConfig_.value("preparedEngine");
        flowConfig_=edited;
        if(!active.isUndefined())flowConfig_.insert("activeCase",active);
        if(!prepared.isUndefined())flowConfig_.insert("preparedHash",prepared);
        if(!engine.isUndefined())flowConfig_.insert("preparedEngine",engine);
    }
    dirty_=true;properties_->setEnabled(false);storeCurrentTab();refreshProjectTabTitle();updateNozzleNotice();
}

bool MainWindow::ensureFlowEnvironment() {
    ensureOpenFoamInstallation();
    if (!hasOfInstallation_) {
        QMessageBox::warning(this,tr("Backend unavailable"),tr("OpenFOAM v2606 is required for compressible flow."));
        return false;
    }
    if (ofEnvironment_.empty()) {
        PosixProcessRunner processes;
        const auto captured=openfoam::Detector::captureEnvironment(ofInstallation_,processes);
        if (!captured) { QMessageBox::warning(this,tr("Backend unavailable"),QString::fromStdString(captured.error().describe()));return false; }
        ofEnvironment_=captured.value();
        ofCaps_=openfoam::Detector::verifyCapabilities(ofInstallation_,ofEnvironment_);
    }
    std::string version;
    for (const auto& [key,value]:ofEnvironment_) if (key=="WM_PROJECT_VERSION") version=value;
    if (version!="v2606" && version!="2606") {
        QMessageBox::warning(this,tr("Backend unavailable"),tr("OpenFOAM v2606 is required for compressible flow."));return false;
    }
    return true;
}

QString MainWindow::flowFingerprint(bool includeRunControls) const {
    auto cfg=flowConfig_;cfg.remove("activeCase");cfg.remove("preparedHash");cfg.remove("preparedEngine");
    if(!includeRunControls) {
        auto solver=cfg.value("solver").toObject();
        for(const auto* key:{"iterations","writeInterval","maxCo","processes","convergence","logInterval"})solver.remove(key);
        if(solver.isEmpty())cfg.remove("solver");else cfg.insert("solver",solver);
    }
    QFile sketch(QString::fromStdString((projectRoot_/"geometry/working-sketch.vsketch.json").string()));
    if (!sketch.open(QIODevice::ReadOnly)) return {};
    const auto inputs=QJsonDocument(cfg).toJson(QJsonDocument::Compact)+sketch.readAll();
    return QString::fromLatin1(QCryptographicHash::hash(inputs,QCryptographicHash::Sha256).toHex());
}

bool MainWindow::ensureFlowCase(bool force) {
    if (!sketchEditor_->sketch().closed()) {
        QMessageBox::warning(this,tr("Invalid fluid domain"),tr("Close the fluid contour before generating its mesh."));return false;
    }
    if (flowConfig_.isEmpty()) { configureFlow(0); if (flowConfig_.isEmpty()) return false; }
    if (!ensureFlowEnvironment() || !saveProject()) return false;
    const auto fingerprint=flowFingerprint();
    const auto previous=flowConfig_.value("activeCase").toString();
    const auto prepared=flowConfig_.value("preparedHash").toString();
    if (!force && flowConfig_.value("preparedEngine").toString()==QString::fromStdString(ofInstallation_.fingerprint())
        && !fingerprint.isEmpty() && (fingerprint==prepared || flowFingerprint(true)==prepared)
        && QFileInfo::exists(previous+"/constant/polyMesh/boundary")) {
        const auto error=updateFlowRunControls(previous,flowConfig_.value("solver").toObject(),ofEnvironment_);
        if(!error.isEmpty()) {QMessageBox::warning(this,tr("Run failed"),error);return false;}
        flowConfig_.insert("preparedHash",fingerprint);storeCurrentTab();
        if(!saveProject())return false;
        console_->appendSolverOutput(tr("Reusing mesh and saved fields: %1. Continuing to iteration %2.\n")
            .arg(previous).arg(flowConfig_.value("solver").toObject().value("iterations").toInt()));
        return true;
    }
    const auto caseDir=generateFlowCase(this,
        QString::fromStdString((projectRoot_/"geometry/working-sketch.vsketch.json").string()),
        flowConfig_,QString::fromStdString(projectRoot_.string()),ofEnvironment_,preferences_.caseStorage);
    if (caseDir.isEmpty()) return false;
    QSaveFile engineMarker(caseDir+"/.vortia-engine");
    const auto engine=QString::fromStdString(ofInstallation_.fingerprint());const auto bytes=engine.toUtf8()+"\n";
    if(!engineMarker.open(QIODevice::WriteOnly)||engineMarker.write(bytes)!=bytes.size()||!engineMarker.commit()) {
        QMessageBox::warning(this,tr("Calculation engine"),tr("The engine identity could not be saved; the new case was retained without activating it."));return false;
    }
    flowConfig_.insert("preparedEngine",engine);
    flowConfig_.insert("activeCase",caseDir);flowConfig_.insert("preparedHash",fingerprint);
    dirty_=true;storeCurrentTab();
    if (!saveProject()) return false;
    console_->appendSolverOutput(tr("New mesh revision: %1\n").arg(caseDir));
    updateNozzleNotice();
    return true;
}

void MainWindow::onGenerateFlowMesh() {
    if (projectSwitchLocked() || useExistingCase_) return;
    if (!ensureFlowCase(true)) return;
    JobController::JobSpec spec;
    spec.caseDir=flowConfig_.value("activeCase").toString().toStdString();
    spec.setup=setup_;spec.caps=ofCaps_;spec.environment=ofEnvironment_;
    spec.meshOnly=true;
    setJobUiRunning(true);
    if (!jobController_->start(spec)) setJobUiRunning(false);
}

void MainWindow::onRun() {
    if (projectSwitchLocked()) return;
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

    if (!useExistingCase_ && projectRoot_.empty() && sketchEditor_->sketch().segmentCount()==0) {
        QMessageBox::information(this,tr("Fluid domain required"),tr("Draw a closed fluid contour or open an existing configured project before calculating."));return;
    }
    if (!useExistingCase_ && sketchEditor_->sketch().segmentCount() && flowConfig_.isEmpty()) {
        configureFlow(1); if (flowConfig_.isEmpty()) return;
    }
    std::filesystem::path caseDir;
    std::string detectedSolver;
    if (!useExistingCase_ && !flowConfig_.isEmpty()) {
        if (!ensureFlowCase()) return;
        caseDir=flowConfig_.value("activeCase").toString().toStdString();
        const auto backendError=stageFlowBackend(QString::fromStdString(caseDir.string()));
        if(!backendError.isEmpty()) {QMessageBox::warning(this,tr("Run failed"),backendError);return;}
        detectedSolver="vortiaCentralFoam";
    } else if (useExistingCase_ && !existingCaseDir_.empty()) {
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
        if (projectRoot_.empty()) {
            auto& temporary = documents_.at(static_cast<std::size_t>(activeProjectTab_)).temporaryCase;
            if (!temporary)
                temporary = std::make_shared<QTemporaryDir>(QDir::tempPath() + QStringLiteral("/vortia-case-XXXXXX"));
            if (!temporary->isValid()) {
                QMessageBox::warning(this, tr("Run failed"), tr("Could not create a temporary case directory."));
                return;
            }
            caseDir = temporary->path().toStdString();
        } else {
            caseDir = projectRoot_ / "case";
        }
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

    // Opt only recognized generated flow cases into the current immutable
    // backend. Arbitrary imported cases and their dictionaries remain untouched.
    if(useExistingCase_&&detectedSolver=="vortiaCentralFoam"&&std::filesystem::exists(caseDir/".vortia-flow-case")) {
        QFile identity(QString::fromStdString((caseDir/".vortia-engine").string()));
        if(identity.open(QIODevice::ReadOnly)) {
            if(QString::fromUtf8(identity.readAll()).trimmed()!=QString::fromStdString(ofInstallation_.fingerprint())) {
                QMessageBox::warning(this,tr("Calculation engine"),tr("This case belongs to another OpenFOAM installation. Open its editable project and generate a new mesh revision; earlier results are retained."));return;
            }
        } else if(!preferences_.backendRoot.isEmpty()) {
            QMessageBox::warning(this,tr("Calculation engine"),tr("This older imported case has no engine identity. Use its editable project to generate a new revision with the selected engine."));return;
        }
        const auto error=stageFlowBackend(QString::fromStdString(caseDir.string()));
        if(!error.isEmpty()){QMessageBox::warning(this,tr("Run failed"),error);return;}
    }
    setJobUiRunning(true);

    JobController::JobSpec spec;
    spec.caseDir = caseDir;
    spec.automaticPreview=preferences_.automaticPreview;
    if(!spec.automaticPreview){lastVtmPath_.clear();viewport_->clearScene();storeCurrentTab();}
    spec.setup = setup_;
    spec.caps = ofCaps_;
    spec.environment = ofEnvironment_;
    spec.solverOverride = detectedSolver;
    if(detectedSolver=="vortiaCentralFoam") {
        QJsonObject runtime{{"installation",QString::fromStdString(ofInstallation_.fingerprint())}};
        QString libraries;
        for(const auto& [key,value]:ofEnvironment_) {
            if(key=="WM_PROJECT_DIR"||key=="WM_PROJECT_VERSION"||key=="WM_OPTIONS"||key=="WM_CXX"||key=="WM_LABEL_SIZE"||key=="WM_PRECISION_OPTION"||key=="FOAM_LIBBIN")
                runtime.insert(QString::fromStdString(key),QString::fromStdString(value));
            if(key=="FOAM_LIBBIN")libraries=QString::fromStdString(value);
        }
        for(const auto* name:{"libOpenFOAM","libfiniteVolume","libfluidThermophysicalModels"})for(const auto* extension:{".dylib",".so"}) {
            QFileInfo library(QDir(libraries).filePath(QString::fromLatin1(name)+extension));
            if(library.exists())runtime.insert(library.fileName(),QString("%1:%2:%3").arg(library.canonicalFilePath()).arg(library.size()).arg(library.lastModified().toMSecsSinceEpoch()));
        }
        const auto key=QCryptographicHash::hash(QJsonDocument(runtime).toJson(QJsonDocument::Compact),QCryptographicHash::Sha256).toHex();
        const auto cache=QDir(QStandardPaths::writableLocation(QStandardPaths::CacheLocation)).filePath("flow-engines");
        spec.environment.emplace_back("VORTIA_SOLVER_RUNTIME_KEY",key.toStdString());
        spec.environment.emplace_back("VORTIA_SOLVER_CACHE_DIR",cache.toStdString());
    }
    if(!useExistingCase_ && detectedSolver=="vortiaCentralFoam") {
        const int selected=flowConfig_.value("solver").toObject().value("processes").toInt(1);
        if(selected==0) {
            QFile metadata(QString::fromStdString((caseDir/"mesh-report.json").string()));double cells=0;
            if(metadata.open(QIODevice::ReadOnly)&&metadata.size()<65536)cells=QJsonDocument::fromJson(metadata.readAll()).object().value("cells").toDouble();
            const auto count=std::isfinite(cells)&&cells>0&&cells<=1000000?static_cast<std::uint64_t>(cells):0;
            spec.processes=recommendedFlowProcesses(count,static_cast<unsigned>(std::max(1,QThread::idealThreadCount())));
            console_->appendSolverOutput(tr("Automatic CPU selection: %1 process(es) for %2 cells.\n").arg(spec.processes).arg(count));
        } else spec.processes=static_cast<unsigned>(selected);
    }
    if (!jobController_->start(spec)) setJobUiRunning(false);
    } catch (const std::exception& e) {
        setJobUiRunning(jobController_->isRunning());
        QMessageBox::warning(this, tr("Run failed"),
                             QString::fromStdString(std::string("exception: ") + e.what()));
    } catch (...) {
        setJobUiRunning(jobController_->isRunning());
        QMessageBox::warning(this, tr("Run failed"), tr("unknown exception"));
    }
}

void MainWindow::onCancel() {
    jobController_->cancel();
    console_->appendSolverOutput(tr("Cancellation requested…\n"));
}

void MainWindow::updateNozzleNotice() {
    const auto caseDir = useExistingCase_ ? existingCaseDir_ :
        (!flowConfig_.isEmpty() ? std::filesystem::path(flowConfig_.value("activeCase").toString().toStdString()) :
        (projectRoot_.empty() ? std::filesystem::path{} : projectRoot_ / "case"));
    const bool generic = !flowConfig_.isEmpty() || std::filesystem::exists(caseDir/".vortia-flow-case");
    const bool nozzle = !caseDir.empty() && openfoam::isNozzlePracticeCase(caseDir);
    nozzleNotice_->setVisible(nozzle || generic);
    flowAssessment_->setVisible(generic);propertiesTabs_->setTabVisible(1,generic);
    propertiesTabs_->setTabVisible(0,!generic);propertiesTabs_->tabBar()->setVisible(!generic);
    if(auto* dock=findChild<QDockWidget*>("vortia-dock-properties"))dock->setWindowTitle(generic?tr("Numerical assessment"):tr("Properties"));
    if (generic) {
        if(!jobController_ || !projectSwitchLocked()) {
            QJsonObject report;
            const bool current=useExistingCase_||flowConfig_.value("preparedHash").toString()==flowFingerprint()
                ||flowConfig_.value("preparedHash").toString()==flowFingerprint(true);
            QFile saved(QString::fromStdString((caseDir/"vortia-convergence.json").string()));
            if(current&&saved.open(QIODevice::ReadOnly)) {
                const auto bytes=saved.read(8193);
                const auto parsed=openfoam::parseConvergenceReport(std::string_view(bytes.constData(),static_cast<std::size_t>(bytes.size())));
                // A report for an unsaved/interrupted sample must never certify a different checkpoint.
                double latest=-1;
                if(!caseDir.empty()&&std::filesystem::is_directory(caseDir))for(const auto& entry:std::filesystem::directory_iterator(caseDir)) {
                    if(!entry.is_directory())continue;
                    try {std::size_t used=0;const auto name=entry.path().filename().string();const auto t=std::stod(name,&used);
                        if(used==name.size()&&std::isfinite(t))latest=std::max(latest,t);
                    }catch(...){}
                }
                if(parsed&&parsed->iteration==latest)report=QJsonDocument::fromJson(bytes).object();
            }
            showFlowAssessment(report);
        }
        QString text=tr("Experimental CFD · numerical checks are automatic. Physical validation and mesh independence remain to be established.");
        if (!useExistingCase_ && flowConfig_.value("activeCase").toString().isEmpty())
            text+=tr(" Generate a mesh from the sketch before calculating.");
        else if (!useExistingCase_ && (flowConfig_.value("preparedHash").toString().isEmpty() ||
                 (flowConfig_.value("preparedHash").toString()!=flowFingerprint() && flowConfig_.value("preparedHash").toString()!=flowFingerprint(true))))
            text+=tr(" Mesh inputs have changed. Generate a new mesh revision before calculating; any loaded results belong to the earlier revision.");
        nozzleNotice_->setText(text);return;
    }
    if (!nozzle) return;
    QString notice = tr("Experimental nozzle: results remain provisional. Convergence and mesh dependence have not been established.");
    const auto balance = openfoam::nozzleMassBalance(caseDir);
    if (balance.available) {
        notice += tr(" Mass imbalance, iterations %1-%2: mean %3%, maximum %4% of the principal inlet flow (%5 samples).")
            .arg(balance.firstIteration, 0, 'g', 9)
            .arg(balance.lastIteration, 0, 'g', 9)
            .arg(balance.meanAbsolutePercent, 0, 'g', 4)
            .arg(balance.maximumAbsolutePercent, 0, 'g', 4)
            .arg(balance.samples);
    } else {
        notice += tr(" No complete aligned mass-balance window is available yet.");
    }
    nozzleNotice_->setText(notice);
}

void MainWindow::showFlowAssessment(const QJsonObject& r) {
    if(!flowAssessment_)return;
    flowAssessment_->setVisible(true);propertiesTabs_->setTabVisible(1,true);
    const auto state=r.value("status").toString();
    QString title=tr("Automatic numerical checks"),detail;
    QString color="#559bb8";
    if(state=="converged") {title=tr("Numerical convergence reached");color="#36a68a";detail=tr("The flow met all numerical criteria for consecutive checks. The final state was saved.");}
    else if(state=="limit") {title=tr("Safety limit reached · not converged");color="#d39541";detail=tr("Results remain provisional. Continue with a higher safety limit, or revise the mesh, boundaries and physical model if progress stalls.");}
    else if(state=="interrupted") {title=tr("Calculation did not finish");color="#d39541";detail=r.value("message").toString().toHtmlEscaped();}
    else if(state=="invalid") {title=tr("Invalid numerical state");color="#d86262";detail=tr("A non-finite or non-positive field was detected. This run cannot be accepted.");}
    else if(state=="warming") {title=tr("Establishing the initial flow");detail=tr("Automatic checks are active. Acceptance starts after the minimum iteration window.");}
    else if(state=="stabilizing") {title=tr("Flow stabilizing");detail=tr("The latest check passed. Vortia is waiting for consecutive successful checks.");}
    else if(state=="checking") {title=tr("Flow still evolving");detail=r.value("automatic").toBool()?tr("Vortia will stop automatically when mass conservation and field stability meet their limits."):tr("Automatic stopping is off. Vortia continues to monitor the flow until the safety limit.");}
    else {detail=tr("Run the calculation to obtain an assessment. Previous results without a matching saved assessment remain unassessed.");}
    QString rows;
    if(r.value("sampled").toBool()) {
        auto metric=[&](const char* label,const char* key,double tolerance) {
            const double v=r.value(key).toDouble();
            return QString("<p>%1<br><b>%2%</b> · %3 %4%</p>").arg(tr(label)).arg(v*100,0,'g',3)
                .arg(v<=tolerance?tr("Pass · limit"):tr("Pending · limit")).arg(tolerance*100,0,'g',3);
        };
        rows+=metric("Global mass balance","globalMass",r.value("massTolerance").toDouble());
        rows+=metric("Local mass conservation","localMass",r.value("massTolerance").toDouble());
        rows+=metric("Normalized update rate","equationError",r.value("equationTolerance").toDouble());
        rows+=metric("Maximum field change","fieldChange",r.value("fieldTolerance").toDouble());
        rows+=QString("<p>%1: <b>%2 / %3</b><br>%4 %5 · %6 %7</p>")
            .arg(tr("Consecutive checks")).arg(r.value("stableSamples").toInt()).arg(r.value("requiredSamples").toInt())
            .arg(tr("Iteration")).arg(r.value("checkedIteration").toDouble(),0,'g',9)
            .arg(tr("Check interval")).arg(r.value("interval").toInt());
    }
    flowAssessment_->setText(QString("<p style='color:%1;font-size:16px'><b>%2</b></p><p>%3</p>%4").arg(color,title,detail,rows));
}

void MainWindow::onJobFinished(bool success, const QString& summary) {
    setJobUiRunning(false);
    updateNozzleNotice();
    if(!success&&propertiesTabs_->isTabVisible(1))showFlowAssessment(QJsonObject{{"status","interrupted"},{"message",summary}});
    setProjectState(success ? tr("Run finished: %1").arg(summary)
                           : tr("Run failed: %1").arg(summary));
    const bool unmet=flowAssessment_&&flowAssessment_->isVisible()&&flowAssessment_->text().contains(tr("Safety limit reached · not converged"));
    console_->appendSolverOutput((success ? unmet?tr("NOT CONVERGED: "):tr("SUCCESS: ") : tr("FAILED: ")) + summary + "\n");
}

bool MainWindow::projectSwitchLocked() const {
    // Keep the lock through delivery of the queued results/finished signals,
    // even after the worker's atomic running flag has become false.
    return cancelAction_ && cancelAction_->isEnabled();
}

void MainWindow::setJobUiRunning(bool running) {
    if(running)if(auto* console=findChild<QDockWidget*>("vortia-dock-console"))console->setVisible(preferences_.showConsoleOnRun);
    if(running&&(!flowConfig_.isEmpty()||propertiesTabs_->isTabVisible(1))) {
        showFlowAssessment(QJsonObject{{"status","warming"}});
        if(preferences_.showAssessmentOnRun)if(auto* dock=findChild<QDockWidget*>("vortia-dock-properties"))dock->show();
    }
    runAction_->setEnabled(!running);
    cancelAction_->setEnabled(running);
    for(auto* action:fieldActions_)action->setEnabled(!running&&!viewport_->availableFields().empty());
    projectTabs_->setEnabled(!running);
    newProjectAction_->setEnabled(!running);
    openProjectAction_->setEnabled(!running);
    openCaseAction_->setEnabled(!running);
}

void MainWindow::onResultsReady(const QString& vtmPath) {
    try {
        lastVtmPath_ = vtmPath;
        storeCurrentTab();
        viewport_->loadVtmFile(vtmPath.toStdString(), fieldCombo_->currentData().toString().toStdString());
        workspaceTabs_->setCurrentWidget(viewport_);
        if(auto* ribbon=findChild<QTabWidget*>("vortia-ribbon"))ribbon->setCurrentIndex(4);
        if(preferences_.showAssessmentOnRun&&propertiesTabs_->isTabVisible(1))
            if(auto* dock=findChild<QDockWidget*>("vortia-dock-properties"))dock->show();
        console_->appendSolverOutput(tr("Loaded results: %1\n").arg(vtmPath));
        setProjectState(tr("Results loaded."));
        updateNozzleNotice();
    } catch (const std::exception& e) {
        console_->appendSolverOutput(tr("Failed to load results: %1\n").arg(QString::fromStdString(e.what())));
    } catch (...) {
        console_->appendSolverOutput(tr("Failed to load results: unknown error\n"));
    }
}



void MainWindow::onViewLatestResult() {
    if(projectSwitchLocked())return;
    const auto path=useExistingCase_?existingCaseDir_:!flowConfig_.isEmpty()
        ?std::filesystem::path(flowConfig_.value("activeCase").toString().toStdString()):projectRoot_/"case";
    if(path.empty()||!std::filesystem::is_regular_file(path/"system/controlDict")) {
        QMessageBox::information(this,tr("Results"),tr("Generate and calculate a case before viewing its results."));return;
    }
    const auto root=QString::fromStdString(path.string());
    const auto exported=currentVtmFile(root);
    if(!exported.isEmpty()){onResultsReady(exported);return;}
    ensureOpenFoamInstallation();if(!hasOfInstallation_) {QMessageBox::warning(this,tr("Results"),tr("An OpenFOAM installation is required to export these results."));return;}
    if(ofEnvironment_.empty()) {
        PosixProcessRunner runner;auto captured=openfoam::Detector::captureEnvironment(ofInstallation_,runner);
        if(!captured){QMessageBox::warning(this,tr("Results"),QString::fromStdString(captured.error().describe()));return;}ofEnvironment_=*captured;
    }
    const auto tool=openfoam::Detector::resolveExecutable("foamToVTK",ofEnvironment_);
    if(!tool){QMessageBox::warning(this,tr("Results"),tr("foamToVTK is unavailable in the selected engine."));return;}
    QProcessEnvironment env;for(const auto& [key,value]:ofEnvironment_)env.insert(QString::fromStdString(key),QString::fromStdString(value));
    QProcess convert;convert.setWorkingDirectory(root);convert.setProcessEnvironment(env);convert.setProcessChannelMode(QProcess::MergedChannels);
    QProgressDialog progress(tr("Preparing result visualization…"),tr("Cancel"),0,0,this);progress.setWindowModality(Qt::WindowModal);progress.setMinimumDuration(0);
    QEventLoop wait;connect(&convert,qOverload<int,QProcess::ExitStatus>(&QProcess::finished),&wait,&QEventLoop::quit);
    connect(&convert,&QProcess::errorOccurred,&wait,&QEventLoop::quit);connect(&progress,&QProgressDialog::canceled,&convert,&QProcess::kill);
    convert.start(QString::fromStdString(tool->string()),{"-latestTime"});wait.exec();progress.close();
    if(convert.state()!=QProcess::NotRunning){convert.kill();convert.waitForFinished(1000);}
    const auto diagnostic=convert.readAll();console_->appendSolverOutput(QString::fromUtf8(diagnostic));
    const auto result=currentVtmFile(root);
    if(convert.exitStatus()==QProcess::NormalExit&&convert.exitCode()==0&&!result.isEmpty())onResultsReady(result);
    else QMessageBox::warning(this,tr("Results"),tr("Visualization export did not finish. The native calculation fields are retained."));
}

void MainWindow::onOpenResults() {
    if (projectSwitchLocked()) return;
    const QString path=QFileDialog::getOpenFileName(this,tr("Open CFD results"),QString(),tr("VTK multiblock (*.vtm)"));
    if (!path.isEmpty()) onResultsReady(path);
}

void MainWindow::refreshFieldTools() {
    const auto available=viewport_->availableFields();const auto selected=fieldCombo_->currentData().toString();
    QSignalBlocker block(fieldCombo_);fieldCombo_->clear();
    for(const auto* preferred:{"p","T","rho","|U|","Ux","Uy","Uz","Mach","nut","nuTilda","yPlus"})
        if(std::find(available.begin(),available.end(),preferred)!=available.end())fieldCombo_->addItem(fieldDisplayName(preferred),preferred);
    for(const auto& name:available)if(fieldCombo_->findData(QString::fromStdString(name))<0)fieldCombo_->addItem(fieldDisplayName(QString::fromStdString(name)),QString::fromStdString(name));
    const int index=fieldCombo_->findData(selected);fieldCombo_->setCurrentIndex(index>=0?index:0);fieldCombo_->setEnabled(!available.empty());
    for(auto* action:fieldActions_)action->setEnabled(!available.empty()&&!projectSwitchLocked());
    if(!available.empty())viewport_->setColorField(fieldCombo_->currentData().toString().toStdString());
}
void MainWindow::onFieldChanged(int /*index*/) {
    if (lastVtmPath_.isEmpty())
        return;
    // Reuse the already loaded VTK dataset: repeated reads previously made
    // each field change much heavier and could leave stale legend artifacts.
    viewport_->setColorField(fieldCombo_->currentData().toString().toStdString());
}

void MainWindow::onFindTool() {
    QDialog dialog(this);dialog.setObjectName("vortia-command-search");dialog.setWindowTitle(tr("Find a tool"));dialog.resize(620,470);
    auto* layout=new QVBoxLayout(&dialog);auto* search=new QLineEdit(&dialog);search->setObjectName("command-query");search->setPlaceholderText(tr("Search by tool name…"));layout->addWidget(search);
    auto* list=new QListWidget(&dialog);list->setObjectName("command-results");list->setIconSize({24,24});layout->addWidget(list,1);
    QSet<QAction*> actions;
    for(auto* menu:findChildren<QMenu*>())for(auto* action:menu->actions())if(!action->isSeparator()&&!action->menu()&&!action->text().isEmpty())actions.insert(action);
    for(auto* button:findChildren<QToolButton*>("vortia-ribbon-button"))if(auto* action=button->defaultAction())if(!action->menu())actions.insert(action);
    auto refresh=[&](){
        list->clear();const auto words=search->text().trimmed().split(' ',Qt::SkipEmptyParts);
        QList<QAction*> sorted=actions.values();std::sort(sorted.begin(),sorted.end(),[](QAction* a,QAction* b){return a->text().localeAwareCompare(b->text())<0;});
        for(auto* action:sorted) {
            if(!action->isEnabled())continue;
            QString label=action->text();label.remove('&');bool match=true;
            for(const auto& word:words)if(!label.contains(word,Qt::CaseInsensitive))match=false;
            if(!match)continue;
            auto* item=new QListWidgetItem(action->icon(),label,list);item->setData(Qt::UserRole,QVariant::fromValue(action));
            item->setToolTip(action->toolTip());
        }
        if(list->count())list->setCurrentRow(0);
    };
    QAction* chosen=nullptr;
    auto choose=[&](){if(auto* item=list->currentItem()){chosen=item->data(Qt::UserRole).value<QAction*>();if(chosen&&chosen->isEnabled())dialog.accept();}};
    connect(search,&QLineEdit::textChanged,&dialog,refresh);connect(search,&QLineEdit::returnPressed,&dialog,choose);
    connect(list,&QListWidget::itemActivated,&dialog,[&](QListWidgetItem*){choose();});
    auto* buttons=new QDialogButtonBox(QDialogButtonBox::Cancel,&dialog);layout->addWidget(buttons);connect(buttons,&QDialogButtonBox::rejected,&dialog,&QDialog::reject);
    refresh();search->setFocus();if(dialog.exec()==QDialog::Accepted&&chosen&&chosen->isEnabled())chosen->trigger();
}

void MainWindow::onPreferences() {
    const auto edited=editApplicationPreferences(this,preferences_,projectSwitchLocked());
    if(!edited)return;
    const bool languageChanged=edited->language!=preferences_.language;
    const bool engineChanged=edited->backendRoot!=preferences_.backendRoot;
    preferences_=*edited;preferences_.save(*settings_);
    if(engineChanged){hasOfInstallation_=false;ofEnvironment_.clear();}
    applyPreferences();updateViewport();
    if(languageChanged)QMessageBox::information(this,tr("Language"),tr("Restart Vortia to apply the selected language."));
}

void MainWindow::applyPreferences() {
    ThemeManager::apply(*qApp,ThemeManager::fromName(preferences_.theme));
    const bool compact=preferences_.density=="compact";
    qApp->setStyleSheet(qApp->styleSheet()+QString("\nQWidget { font-size:%1px; }").arg(12*preferences_.interfaceScale/100));
    if(auto* ribbon=findChild<QTabWidget*>("vortia-ribbon"))ribbon->setMinimumHeight(compact?100:126);
    for(auto* button:findChildren<QToolButton*>("vortia-ribbon-button")) {
        button->setIconSize(QSize(compact?22:28,compact?22:28));button->setMinimumSize(compact?QSize(74,52):QSize(78,69));
    }
    console_->setMaximumBlockCount(preferences_.consoleLines);
    viewport_->setNavigationPreferences(preferences_.cfdZoomSensitivity,preferences_.cfdPanSensitivity);
    sketchEditor_->setInteractionPreferences(preferences_.snapToGrid,preferences_.snapSpacing,
        preferences_.showGrid,preferences_.showDimensions,preferences_.zoomSensitivity);
}

void MainWindow::onToggleTheme() {
    const QString current = settings_->value(QStringLiteral("theme"), QStringLiteral("dark")).toString();
    const QString next = current == QStringLiteral("dark") ? QStringLiteral("light")
                                                           : QStringLiteral("dark");
    settings_->setValue(QStringLiteral("theme"), next);
    preferences_.theme=next;applyPreferences();
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
    if(!preferences_.backendRoot.isEmpty()) {
        auto installed=openfoam::Detector::probe(preferences_.backendRoot.toStdString());
        if(installed){ofInstallation_=*installed;hasOfInstallation_=true;ofEnvironment_.clear();}
        return;
    }
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
    storeCurrentTab();
    bool pending = false;
    for (const auto& doc : documents_)
        pending = pending || doc.modified;
    if (pending && QMessageBox::question(this, tr("Unsaved projects"),
         tr("Some projects have unsaved changes. Exit without saving?"),
         QMessageBox::Yes | QMessageBox::No, QMessageBox::No) != QMessageBox::Yes) {
        event->ignore();
        return;
    }
    saveSettings();
    QMainWindow::closeEvent(event);
}

} // namespace vortia
