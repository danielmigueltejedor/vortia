#include "vortia/ui/MainWindow.hpp"
#include "vortia/core/Project.hpp"
#include "vortia/physics/CaseSetupJson.hpp"

#include <gtest/gtest.h>
#include <QApplication>
#include <QAction>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QDir>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QKeyEvent>
#include <QMessageBox>
#include <QPushButton>
#include <QTemporaryDir>
#include <QThread>
#include <QTimer>
#include <QTranslator>

namespace vortia {
// Exercise document operations directly without native file-picker automation.
struct WorkspaceTestAccess {
    static void add(MainWindow& w, const QString& name) { w.addProjectTab(name, w.setup_); }
    static std::string setupJson(MainWindow& w) { return caseSetupToJson(w.setup_); }
    static void close(MainWindow& w, int index) { w.closeProjectTab(index); }
    static void lock(MainWindow& w, bool value) { w.setJobUiRunning(value); }
    static bool open(MainWindow& w, const QString& path) { return w.openProjectDirectory(path); }
    static bool save(MainWindow& w) { return w.saveProject(); }
    static SketchEditorDialog& editor(MainWindow& w) { return *w.sketchEditor_; }
    static void edit(MainWindow& w, const Sketch2D& s, NozzleDimensions dims) {
        w.sketchEditor_->setNozzleDimensions(dims);
        w.sketchEditor_->setSketch(s);
        emit w.sketchEditor_->sketchChanged();
    }
    static bool dirty(const MainWindow& w) { return w.dirty_; }
    static void select(MainWindow& w, int index) { w.projectTabs_->setCurrentIndex(index); }
    static int active(const MainWindow& w) { return w.activeProjectTab_; }
    static std::size_t count(const MainWindow& w) { return w.documents_.size(); }
    static bool newEnabled(MainWindow& w) { return w.newProjectAction_->isEnabled(); }
    static double endTime(MainWindow& w) { return w.setup_.time.endTime; }
    static void setEndTime(MainWindow& w, double value) {
        auto setup = w.setup_; setup.time.endTime = value; w.onApplySetup(setup);
    }
};
} // namespace vortia

using namespace vortia;

namespace {
void writeFile(const QString& path, const QByteArray& contents) {
    QFile f(path);
    ASSERT_TRUE(f.open(QIODevice::WriteOnly));
    ASSERT_EQ(f.write(contents), contents.size());
}

QByteArray readFile(const QString& path) {
    QFile f(path);
    if (!f.open(QIODevice::ReadOnly)) return {};
    return f.readAll();
}

void expectShape(const Sketch2D& actual, const Sketch2D& expected) {
    ASSERT_EQ(actual.points().size(), expected.points().size());
    EXPECT_EQ(actual.closed(), expected.closed());
    for (std::size_t i = 0; i < expected.points().size(); ++i) {
        EXPECT_DOUBLE_EQ(actual.points()[i].xMm, expected.points()[i].xMm);
        EXPECT_DOUBLE_EQ(actual.points()[i].yMm, expected.points()[i].yMm);
    }
}

TEST(SketchPersistence, RoundTripDimensionsAndExactSegmentEdits) {
    QTemporaryDir dir;
    NozzleDimensions dims{90, 40, 140, 160, 460};
    auto shape = makeNozzleProfile(dims).value();
    ASSERT_TRUE(shape.splitSegment(0));
    ASSERT_TRUE(shape.setSegmentLength(0, 123.456));
    SketchEditorDialog original;
    original.setSketch(shape);
    original.setNozzleDimensions(dims);
    const auto path = dir.filePath("geometry/working-sketch.vsketch.json");
    ASSERT_TRUE(original.writeSketchFile(path));
    SketchEditorDialog restored;
    ASSERT_TRUE(restored.readSketchFile(path));
    expectShape(restored.sketch(), shape);
    EXPECT_NEAR(restored.sketch().segmentLength(0), 123.456, 1e-9);
    EXPECT_DOUBLE_EQ(restored.nozzleDimensions().divergingLengthMm, 460);
}

TEST(SketchPersistence, MalformedFilesNeverChangeCurrentDocument) {
    QTemporaryDir dir;
    SketchEditorDialog editor;
    const auto shape = makeRectangleSketch(200, 90).value();
    editor.setSketch(shape);
    editor.setNozzleDimensions({90, 40, 140, 160, 460});
    const auto path = dir.filePath("bad.json");
    const std::vector<QByteArray> invalid{
        "{", R"({"schemaVersion":1,"units":"mm"})",
        R"({"schemaVersion":1,"units":"mm","vertices":{},"closed":false})",
        R"({"schemaVersion":1,"units":"mm","vertices":[],"closed":"false"})",
        R"({"schemaVersion":1.5,"units":"mm","vertices":[],"closed":false})",
        R"({"schemaVersion":1,"units":"m","vertices":[],"closed":false})",
        R"({"schemaVersion":1,"units":"mm","vertices":[[0,0],[0,0]],"closed":false})",
        R"({"schemaVersion":1,"units":"mm","vertices":[[0,0]],"closed":true})",
        R"({"schemaVersion":1,"units":"mm","vertices":[[1e300,0]],"closed":false})",
        R"({"schemaVersion":1,"units":"mm","vertices":[],"closed":false,"nozzleDimensions":{}})"
    };
    for (const auto& contents : invalid) {
        SCOPED_TRACE(contents.toStdString());
        writeFile(path, contents);
        EXPECT_FALSE(editor.readSketchFile(path));
        expectShape(editor.sketch(), shape);
        EXPECT_DOUBLE_EQ(editor.nozzleDimensions().throatDiameterMm, 40);
    }
}

TEST(SketchPersistence, LegacyFilesResetDimensionsAndEmptyProfileRoundTrips) {
    QTemporaryDir dir;
    const auto path = dir.filePath("legacy.json");
    writeFile(path, R"({"schemaVersion":1,"units":"mm","vertices":[],"closed":false})");
    SketchEditorDialog editor;
    editor.setNozzleDimensions({90, 40, 140, 160, 460});
    ASSERT_TRUE(editor.readSketchFile(path));
    EXPECT_DOUBLE_EQ(editor.nozzleDimensions().throatDiameterMm, NozzleDimensions{}.throatDiameterMm);
    EXPECT_TRUE(editor.sketch().points().empty());
    ASSERT_TRUE(editor.writeSketchFile(path));
    ASSERT_TRUE(editor.readSketchFile(path));
}

TEST(SketchPersistence, NozzleDimensionDialogRegeneratesAndSavesChangedSizes) {
    SketchEditorDialog editor;
    QPushButton* nozzleButton = nullptr;
    for (auto* b : editor.findChildren<QPushButton*>())
        if (b->text() == SketchEditorDialog::tr("Parametric nozzle…")) nozzleButton = b;
    ASSERT_NE(nozzleButton, nullptr);
    const auto create = [&](const std::vector<double>& values) {
        QTimer answer;
        QObject::connect(&answer, &QTimer::timeout, [&] {
            if (auto* box = qobject_cast<QMessageBox*>(QApplication::activeModalWidget())) {
                box->button(QMessageBox::Yes)->click();
            } else if (auto* dialog = qobject_cast<QDialog*>(QApplication::activeModalWidget())) {
                const auto controls = dialog->findChildren<QDoubleSpinBox*>();
                EXPECT_EQ(controls.size(), static_cast<qsizetype>(values.size()));
                for (qsizetype i = 0; i < controls.size() && static_cast<std::size_t>(i) < values.size(); ++i)
                    controls[i]->setValue(values[static_cast<std::size_t>(i)]);
                dialog->accept();
            }
        });
        answer.start(1);
        nozzleButton->click();
    };
    int changes = 0;
    QObject::connect(&editor, &SketchEditorDialog::sketchChanged, [&] { ++changes; });
    create({90, 40, 140, 160, 460});
    expectShape(editor.sketch(), makeNozzleProfile({90, 40, 140, 160, 460}).value());
    create({110, 55, 130, 180, 430});
    expectShape(editor.sketch(), makeNozzleProfile({110, 55, 130, 180, 430}).value());
    EXPECT_EQ(changes, 2);
    QTemporaryDir dir;
    ASSERT_TRUE(editor.writeSketchFile(dir.filePath("nozzle.json")));
    SketchEditorDialog restored;
    ASSERT_TRUE(restored.readSketchFile(dir.filePath("nozzle.json")));
    expectShape(restored.sketch(), editor.sketch());
    EXPECT_DOUBLE_EQ(restored.nozzleDimensions().throatDiameterMm, 55);
}

TEST(Workspace, IndependentTabsSaveReopenAndDeduplicate) {
    QTemporaryDir directory;
    const auto root = std::filesystem::path(directory.path().toStdString()) / "project";
    auto project = Project::create(root, "project");
    ASSERT_TRUE(project);
    MainWindow window;
    project->setSetupJson(WorkspaceTestAccess::setupJson(window));
    ASSERT_TRUE(project->save());
    ASSERT_TRUE(WorkspaceTestAccess::open(window, QString::fromStdString(root.string())));
    const NozzleDimensions dims{90, 40, 140, 160, 460};
    const auto nozzle = makeNozzleProfile(dims).value();
    WorkspaceTestAccess::edit(window, nozzle, dims);
    WorkspaceTestAccess::setEndTime(window, 2.5);
    EXPECT_TRUE(WorkspaceTestAccess::dirty(window));
    WorkspaceTestAccess::add(window, "other");
    const auto rectangle = makeRectangleSketch(55, 25).value();
    WorkspaceTestAccess::edit(window, rectangle, {});
    WorkspaceTestAccess::select(window, 1);
    expectShape(WorkspaceTestAccess::editor(window).sketch(), nozzle);
    EXPECT_DOUBLE_EQ(WorkspaceTestAccess::endTime(window), 2.5);
    ASSERT_TRUE(WorkspaceTestAccess::save(window));
    EXPECT_FALSE(WorkspaceTestAccess::dirty(window));
    const auto count = WorkspaceTestAccess::count(window);
    ASSERT_TRUE(WorkspaceTestAccess::open(window, QString::fromStdString(root.string()) + "/."));
    EXPECT_EQ(WorkspaceTestAccess::count(window), count);
    WorkspaceTestAccess::select(window, 2);
    expectShape(WorkspaceTestAccess::editor(window).sketch(), rectangle);
    WorkspaceTestAccess::close(window, 1); // closing an inactive saved tab preserves the active one
    EXPECT_EQ(WorkspaceTestAccess::active(window), 1);
    expectShape(WorkspaceTestAccess::editor(window).sketch(), rectangle);
    MainWindow reopened;
    ASSERT_TRUE(WorkspaceTestAccess::open(reopened, QString::fromStdString(root.string())));
    expectShape(WorkspaceTestAccess::editor(reopened).sketch(), nozzle);
    EXPECT_DOUBLE_EQ(WorkspaceTestAccess::editor(reopened).nozzleDimensions().throatDiameterMm, 40);
    EXPECT_DOUBLE_EQ(WorkspaceTestAccess::endTime(reopened), 2.5);
    EXPECT_FALSE(WorkspaceTestAccess::dirty(reopened));
}

TEST(Workspace, RunLockRejectsNewOpenSwitchAndClose) {
    MainWindow window;
    WorkspaceTestAccess::add(window, "second");
    WorkspaceTestAccess::lock(window, true);
    EXPECT_FALSE(WorkspaceTestAccess::newEnabled(window));
    ASSERT_TRUE(QMetaObject::invokeMethod(&window, "onNewProject", Qt::DirectConnection));
    ASSERT_TRUE(QMetaObject::invokeMethod(&window, "onOpenProject", Qt::DirectConnection));
    ASSERT_TRUE(QMetaObject::invokeMethod(&window, "onOpenExistingCase", Qt::DirectConnection));
    WorkspaceTestAccess::add(window, "blocked");
    EXPECT_FALSE(WorkspaceTestAccess::open(window, "/missing"));
    WorkspaceTestAccess::select(window, 0);
    WorkspaceTestAccess::close(window, 1);
    EXPECT_EQ(WorkspaceTestAccess::count(window), 2U);
    EXPECT_EQ(WorkspaceTestAccess::active(window), 1);
    WorkspaceTestAccess::lock(window, false);
    EXPECT_TRUE(WorkspaceTestAccess::newEnabled(window));
    WorkspaceTestAccess::select(window, 0);
    EXPECT_EQ(WorkspaceTestAccess::active(window), 0);
}

TEST(Workspace, CancelClosingDirtyTabKeepsDrawing) {
    MainWindow window;
    const auto shape = makeCircleSketch(55).value();
    WorkspaceTestAccess::edit(window, shape, {});
    QTimer::singleShot(0, [] {
        if (auto* box = qobject_cast<QMessageBox*>(QApplication::activeModalWidget())) box->button(QMessageBox::No)->click();
    });
    WorkspaceTestAccess::close(window, 0);
    EXPECT_EQ(WorkspaceTestAccess::count(window), 1U);
    EXPECT_TRUE(WorkspaceTestAccess::dirty(window));
    expectShape(WorkspaceTestAccess::editor(window).sketch(), shape);
}

TEST(Workspace, InvalidSavedSketchDoesNotOpenOrOverwriteCurrentDocument) {
    QTemporaryDir dir;
    const auto root = std::filesystem::path(dir.path().toStdString());
    auto project = Project::create(root, "broken");
    ASSERT_TRUE(project);
    MainWindow window;
    project->setSetupJson(WorkspaceTestAccess::setupJson(window));
    ASSERT_TRUE(project->save());
    const auto shape = makeRectangleSketch(30, 20).value();
    WorkspaceTestAccess::edit(window, shape, {});
    QDir().mkpath(dir.filePath("geometry"));
    const auto sketchFile = dir.filePath("geometry/working-sketch.vsketch.json");
    writeFile(sketchFile, "{broken sketch");
    const auto manifest = readFile(dir.filePath("project.vortia"));
    QTimer::singleShot(0, [] {
        if (auto* box = qobject_cast<QMessageBox*>(QApplication::activeModalWidget())) box->accept();
    });
    EXPECT_FALSE(WorkspaceTestAccess::open(window, dir.path()));
    EXPECT_EQ(WorkspaceTestAccess::count(window), 1U);
    expectShape(WorkspaceTestAccess::editor(window).sketch(), shape);
    EXPECT_TRUE(WorkspaceTestAccess::dirty(window));
    EXPECT_EQ(readFile(sketchFile), "{broken sketch");
    EXPECT_EQ(readFile(dir.filePath("project.vortia")), manifest);
}

TEST(Workspace, EmbeddedEditorEscapeReturnsToViewport) {
    MainWindow window;
    auto* tabs = window.findChild<QTabWidget*>("vortia-editor-tabs");
    ASSERT_NE(tabs, nullptr);
    tabs->setCurrentIndex(1);
    WorkspaceTestAccess::editor(window).reject();
    EXPECT_EQ(tabs->currentIndex(), 0);
}

TEST(JobController, CanStartAgainAfterCompletedWorker) {
    JobController controller;
    int finished = 0;
    QObject::connect(&controller, &JobController::jobFinished, &controller,
                     [&](bool success, const QString&) { EXPECT_FALSE(success); ++finished; });
    for (int run = 1; run <= 3; ++run) {
        ASSERT_TRUE(controller.start({})); // no backend: deterministic failure, no CFD run
        QElapsedTimer timer;
        timer.start();
        while (finished < run && timer.elapsed() < 5000) {
            QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
            QThread::msleep(1);
        }
        ASSERT_EQ(finished, run);
        EXPECT_FALSE(controller.isRunning());
    }
}

TEST(WorkspaceResources, RealTranslationContextsAndExplorerIcons) {
    QTranslator translator;
    ASSERT_TRUE(translator.load(QStringLiteral(VORTIA_TEST_QM)));
    qApp->installTranslator(&translator);
    EXPECT_EQ(MainWindow::tr("File"), QStringLiteral("Archivo"));
    EXPECT_EQ(SketchEditorDialog::tr("Nozzle dimensions"), QStringLiteral("Cotas de la tobera"));
    QDialogButtonBox buttons(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    EXPECT_EQ(buttons.button(QDialogButtonBox::Ok)->text(), QStringLiteral("Aceptar"));
    EXPECT_EQ(buttons.button(QDialogButtonBox::Cancel)->text(), QStringLiteral("Cancelar"));
    ProjectExplorerPanel explorer;
    EXPECT_EQ(explorer.topLevelItem(0)->text(0), QStringLiteral("Geometría"));
    const QStringList ids{"geometry", "mesh", "material", "boundary", "solver", "view"};
    for (int i = 0; i < ids.size(); ++i) {
        const QIcon expected(":/vortia/icons/" + ids[i] + ".svg");
        ASSERT_FALSE(expected.pixmap(24).isNull());
        EXPECT_EQ(explorer.topLevelItem(i)->icon(0).pixmap(24).toImage(), expected.pixmap(24).toImage());
    }
    qApp->removeTranslator(&translator);
}
} // namespace

int main(int argc, char** argv) {
    QApplication app(argc, argv);
    QTemporaryDir settings;
    QSettings::setDefaultFormat(QSettings::IniFormat);
    QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, settings.path());
    ::testing::InitGoogleTest(&argc, argv);
    return RUN_ALL_TESTS();
}
