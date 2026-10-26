#include "vortia/ui/MainWindow.hpp"
#include "vortia/core/Project.hpp"
#include "vortia/physics/CaseSetupJson.hpp"
#include "vortia/ui/ResultFiles.hpp"
#include "vortia/ui/FlowSetup.hpp"
#include "vortia/ui/ConsolePanel.hpp"
#include "vortia/solver/openfoam/Detector.hpp"
#include <QJsonDocument>
#include <QJsonArray>
#include <QProcess>
#include <QProcessEnvironment>
#include <QRegularExpression>
#include <QLineEdit>
#include <QCheckBox>
#include <QSpinBox>
#include <QButtonGroup>
#include <QToolButton>
#include <QMenu>
#include <QSettings>
#include <map>
#include <numbers>
#include <QListWidget>
#include <QStackedWidget>

#include <gtest/gtest.h>
#include <algorithm>
#include <cmath>
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
#include <QMouseEvent>
#include <QNativeGestureEvent>
#include <QPointingDevice>
#include <QWheelEvent>
#include <QMessageBox>
#include <QPushButton>
#include <QTemporaryDir>
#include <QThread>
#include <QTimer>
#include <QTranslator>
#include <QTableWidget>
#include <vtkNew.h>
#include <vtkImageData.h>
#include <vtkMultiBlockDataSet.h>
#include <vtkXMLMultiBlockDataWriter.h>
#include <vtkDoubleArray.h>
#include <vtkPointData.h>
#include <vtkAppendFilter.h>
#include <vtkUnstructuredGrid.h>
#include <vtkRenderer.h>
#include <vtkRendererCollection.h>
#include <vtkRenderWindow.h>
#include <vtkActorCollection.h>
#include <vtkCamera.h>
#include "vortia/ui/FieldTools.hpp"
#include "vortia/ui/RunMonitor.hpp"
#include "vortia/ui/BoundaryMapping.hpp"
#include "vortia/ui/BoundaryPreview.hpp"
#include "vortia/ui/MeshRegionsWidget.hpp"
#include "vortia/ui/ThemeManager.hpp"
#include <QClipboard>
#include <QMimeData>

namespace vortia {
// Exercise document operations directly without native file-picker automation.
struct WorkspaceTestAccess {
    static QString fingerprint(MainWindow& w,bool legacy=false) {return w.flowFingerprint(legacy);}
    static void configure(MainWindow& w) {w.configureFlow(3);}
    static void result(MainWindow& w,const QString& path) {w.onResultsReady(path);}
    static auto bounds(MainWindow& w) {return w.viewport_->fieldBounds();}
    static int actors(MainWindow& w) {
        return w.viewport_->renderWindow()->GetRenderers()->GetFirstRenderer()->GetActors()->GetNumberOfItems();
    }
    static void flow(MainWindow& w,const QJsonObject& value) {w.flowConfig_=value;w.dirty_=true;w.storeCurrentTab();}
    static QJsonObject flow(const MainWindow& w) {return w.flowConfig_;}
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
    static QString title(const MainWindow& w, int i) { return w.projectTabs_->tabText(i); }
    static bool external(const MainWindow& w) { return w.useExistingCase_; }
    static bool physicsEnabled(const MainWindow& w) { return w.properties_->isEnabled(); }
    static void applyCavity(MainWindow& w) { auto setup=w.setup_; setup.time.endTime=123; w.onApplySetup(setup); }
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
    for (std::size_t i = 0; i < expected.segmentCount(); ++i)
        EXPECT_DOUBLE_EQ(actual.segmentBulge(i), expected.segmentBulge(i));
}

TEST(ResultFiles, SeriesUsesActualTimeAndFallbackUsesNumericIndex) {
    QTemporaryDir dir;
    writeFile(dir.filePath("case_9.vtm"),"test");
    writeFile(dir.filePath("case_10.vtm"),"test");
    EXPECT_EQ(latestVtmFile(dir.path()),dir.filePath("case_10.vtm"));
    writeFile(dir.filePath("case.vtm.series"),R"({"files":[{"name":"case_9.vtm","time":20},{"name":"case_10.vtm","time":10},{"name":"../outside.vtm","time":99},{"name":"missing.vtm","time":100}]})");
    EXPECT_EQ(latestVtmFile(dir.path()),dir.filePath("case_9.vtm"));
}

TEST(SketchNavigation, NativePinchAndPanKeepGeometryAndCameraStableWhileDrawing) {
    SketchEditorDialog editor;
    editor.resize(1000, 700);
    editor.show();
    QApplication::processEvents();
    auto* canvas = editor.findChild<QWidget*>("vortia-sketch-canvas");
    auto* draw = editor.findChild<QPushButton*>("vortia-sketch-draw");
    auto* select = editor.findChild<QPushButton*>("vortia-sketch-select");
    auto* fit = editor.findChild<QPushButton*>("vortia-sketch-fit");
    ASSERT_TRUE(canvas && draw && select && fit);
    int changes = 0;
    QObject::connect(&editor, &SketchEditorDialog::sketchChanged, [&] { ++changes; });
    const double scale = std::min((canvas->width()-65.0)/200.0, (canvas->height()-65.0)/140.0);
    const QPointF center(canvas->width()/2.0, canvas->height()/2.0);
    QNativeGestureEvent pinch(Qt::ZoomNativeGesture, QPointingDevice::primaryPointingDevice(),
        2, center, center, center, 1.0, {});
    QApplication::sendEvent(canvas, &pinch);
    QWheelEvent swipe(center, center, QPoint(40,-20), {}, Qt::NoButton, Qt::NoModifier,
                      Qt::ScrollUpdate, false, Qt::MouseEventSynthesizedBySystem);
    QApplication::sendEvent(canvas, &swipe);
    const auto mouse = [&](QEvent::Type type, QPointF at, Qt::MouseButton button, Qt::MouseButtons buttons) {
        QMouseEvent event(type, at, at, at, button, buttons, Qt::NoModifier);
        QApplication::sendEvent(canvas, &event);
    };
    mouse(QEvent::MouseButtonPress, {80,50}, Qt::MiddleButton, Qt::MiddleButton);
    mouse(QEvent::MouseMove, {110,65}, Qt::NoButton, Qt::MiddleButton);
    mouse(QEvent::MouseButtonRelease, {110,65}, Qt::MiddleButton, Qt::NoButton);
    QApplication::processEvents();
    EXPECT_EQ(changes, 0);
    EXPECT_TRUE(editor.sketch().points().empty());
    // Accessibility can toggle a checkable button without emitting clicked.
    draw->setChecked(true);
    const auto appendAt = [&](QPointF at) {
        mouse(QEvent::MouseButtonPress, at, Qt::LeftButton, Qt::LeftButton);
        mouse(QEvent::MouseButtonRelease, at, Qt::LeftButton, Qt::NoButton);
        QApplication::processEvents(); // repaint must not refit the camera
        const auto& point = editor.sketch().points().back();
        EXPECT_DOUBLE_EQ(point.xMm, std::round(100+(at.x()-center.x()-70)/(2*scale)));
        EXPECT_DOUBLE_EQ(point.yMm, std::round((center.y()-at.y()-5)/(2*scale)));
    };
    appendAt(center + QPointF(80,40));
    appendAt(center + QPointF(140,70));
    EXPECT_EQ(changes, 2);
    QKeyEvent escape(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
    QApplication::sendEvent(canvas, &escape);
    EXPECT_TRUE(select->isChecked()); EXPECT_FALSE(draw->isChecked());
    fit->click(); QApplication::processEvents();
    EXPECT_EQ(changes, 2);
}

TEST(SketchNavigation, CadToolsHaveAccessibleIconsAndExclusiveModes) {
    SketchEditorDialog editor;
    for (const auto* id : {"select", "draw", "pan", "fit", "rectangle", "circle", "nozzle",
                          "add", "move", "split", "dimension", "fillet", "radius", "close-profile", "export"}) {
        auto* tool = editor.findChild<QPushButton*>(QStringLiteral("vortia-sketch-")+id);
        ASSERT_NE(tool, nullptr) << id;
        EXPECT_FALSE(tool->icon().isNull()) << id;
        EXPECT_FALSE(tool->accessibleName().isEmpty()) << id;
        EXPECT_FALSE(tool->toolTip().isEmpty()) << id;
        EXPECT_TRUE(tool->text().isEmpty()) << id;
    }
    auto* pan = editor.findChild<QPushButton*>("vortia-sketch-pan");
    auto* draw = editor.findChild<QPushButton*>("vortia-sketch-draw");
    auto* select = editor.findChild<QPushButton*>("vortia-sketch-select");
    pan->click(); EXPECT_TRUE(pan->isChecked()); EXPECT_FALSE(select->isChecked());
    draw->click(); EXPECT_TRUE(draw->isChecked()); EXPECT_FALSE(pan->isChecked());
    editor.setSketch({}); EXPECT_TRUE(select->isChecked()); EXPECT_FALSE(draw->isChecked());
}

TEST(SketchNavigation, ReadOnlyAllowsSelectingAndPanningButNeverMovesVertices) {
    SketchEditorDialog editor;
    const auto shape = makeRectangleSketch(200,100).value();
    editor.setSketch(shape); editor.resize(1000,700); editor.show();
    QApplication::processEvents(); editor.setReadOnly(true);
    auto* canvas = editor.findChild<QWidget*>("vortia-sketch-canvas");
    ASSERT_NE(canvas,nullptr);
    const double scale = std::min((canvas->width()-65.0)/200.0, (canvas->height()-65.0)/170.0);
    const QPointF vertex(canvas->width()/2.0-100*scale, canvas->height()/2.0+15*scale);
    int changes=0;
    QObject::connect(&editor,&SketchEditorDialog::sketchChanged,[&] { ++changes; });
    QMouseEvent down(QEvent::MouseButtonPress,vertex,vertex,vertex,Qt::LeftButton,Qt::LeftButton,Qt::NoModifier);
    QApplication::sendEvent(canvas,&down);
    const QPointF moved=vertex+QPointF(40,30);
    QMouseEvent drag(QEvent::MouseMove,moved,moved,moved,Qt::NoButton,Qt::LeftButton,Qt::NoModifier);
    QApplication::sendEvent(canvas,&drag);
    QMouseEvent up(QEvent::MouseButtonRelease,moved,moved,moved,Qt::LeftButton,Qt::NoButton,Qt::NoModifier);
    QApplication::sendEvent(canvas,&up);
    expectShape(editor.sketch(),shape); EXPECT_EQ(changes,0);
    auto* draw=editor.findChild<QPushButton*>("vortia-sketch-draw");
    draw->click(); EXPECT_FALSE(draw->isChecked());
    auto* pan=editor.findChild<QPushButton*>("vortia-sketch-pan");
    EXPECT_TRUE(pan->isEnabled()); pan->click(); EXPECT_TRUE(pan->isChecked());
    editor.setReadOnly(false); EXPECT_TRUE(draw->isEnabled());
}

TEST(Workspace, ExternalCaseLoadsSketchAndCannotBecomeCavityProject) {
    QTemporaryDir dir;
    ASSERT_TRUE(QDir().mkpath(dir.filePath("system")));
    const QByteArray control="application rhoCentralFoam;\n";
    writeFile(dir.filePath("system/controlDict"),control);
    auto shape=makeRectangleSketch(100,50).value();
    ASSERT_TRUE(shape.filletVertex(1,10));
    SketchEditorDialog editor;editor.setSketch(shape);
    ASSERT_TRUE(editor.writeSketchFile(dir.filePath("geometry/working-sketch.vsketch.json")));
    MainWindow window;
    ASSERT_TRUE(WorkspaceTestAccess::open(window,dir.path()));
    EXPECT_TRUE(WorkspaceTestAccess::external(window));
    EXPECT_FALSE(WorkspaceTestAccess::physicsEnabled(window));
    EXPECT_TRUE(WorkspaceTestAccess::editor(window).isEnabled());
    EXPECT_TRUE(WorkspaceTestAccess::editor(window).isReadOnly());
    EXPECT_TRUE(WorkspaceTestAccess::editor(window).findChild<QPushButton*>("vortia-sketch-pan")->isEnabled());
    EXPECT_FALSE(WorkspaceTestAccess::editor(window).findChild<QPushButton*>("vortia-sketch-draw")->isEnabled());
    expectShape(WorkspaceTestAccess::editor(window).sketch(),shape);
    WorkspaceTestAccess::applyCavity(window);
    EXPECT_FALSE(WorkspaceTestAccess::dirty(window));
    QTimer::singleShot(0,[] { if (auto* d=qobject_cast<QDialog*>(QApplication::activeModalWidget())) d->accept(); });
    EXPECT_FALSE(WorkspaceTestAccess::save(window));
    EXPECT_FALSE(QFileInfo::exists(dir.filePath("project.vortia")));
    EXPECT_EQ(readFile(dir.filePath("system/controlDict")),control);
    WorkspaceTestAccess::select(window,0);
    EXPECT_TRUE(WorkspaceTestAccess::physicsEnabled(window));
    EXPECT_TRUE(WorkspaceTestAccess::editor(window).isEnabled());
    EXPECT_FALSE(WorkspaceTestAccess::editor(window).isReadOnly());
}

TEST(SketchPersistence, CircularArcsRoundTripAndMalformedArcsAreAtomic) {
    QTemporaryDir dir;
    auto shape = makeRectangleSketch(100, 50).value();
    ASSERT_TRUE(shape.filletVertex(1, 10));
    ASSERT_TRUE(shape.splitSegment(1));
    SketchEditorDialog editor;
    editor.setSketch(shape);
    const auto path = dir.filePath("arcs.json");
    ASSERT_TRUE(editor.writeSketchFile(path));
    const QByteArray saved = readFile(path);
    EXPECT_TRUE(saved.contains("segmentBulges"));
    SketchEditorDialog loaded;
    ASSERT_TRUE(loaded.readSketchFile(path));
    expectShape(loaded.sketch(), shape);
    EXPECT_NEAR(loaded.sketch().signedArea(), shape.signedArea(), 1e-9);
    for (const auto& contents : std::vector<QByteArray>{
        R"({"schemaVersion":2,"units":"mm","closed":false,"vertices":[[0,0],[10,10]]})",
        R"({"schemaVersion":2,"units":"mm","closed":false,"vertices":[[0,0],[10,10]],"segmentBulges":[]})",
        R"({"schemaVersion":2,"units":"mm","closed":false,"vertices":[[0,0],[10,10]],"segmentBulges":[2]})",
        R"({"schemaVersion":2,"units":"mm","closed":false,"vertices":[[0,0],[10,10]],"segmentBulges":["0.2"]})",
        R"({"schemaVersion":1,"units":"mm","closed":false,"vertices":[[0,0],[10,10]],"segmentBulges":[0.2]})"
    }) {
        writeFile(path, contents);
        EXPECT_FALSE(loaded.readSketchFile(path));
        expectShape(loaded.sketch(), shape);
    }
}

TEST(SketchPersistence, RoundingAndRadiusButtonsEditAndPersistActualArcs) {
    SketchEditorDialog editor;
    editor.setSketch(makeRectangleSketch(100, 50).value());
    auto* table = editor.findChild<QTableWidget*>();
    auto* rounding = editor.findChild<QPushButton*>("vortia-sketch-fillet");
    auto* radius = editor.findChild<QPushButton*>("vortia-sketch-radius");
    ASSERT_NE(table, nullptr);
    ASSERT_NE(rounding, nullptr);
    ASSERT_NE(radius, nullptr);
    table->cellClicked(1, 0);
    const auto enterRadius = [](QPushButton* button, double value) {
        QTimer::singleShot(0, [value] {
            auto* dialog = qobject_cast<QDialog*>(QApplication::activeModalWidget());
            ASSERT_NE(dialog, nullptr);
            auto* spin = dialog->findChild<QDoubleSpinBox*>();
            ASSERT_NE(spin, nullptr);
            spin->setValue(value);
            dialog->accept();
        });
        button->click();
    };
    int changes = 0;
    QObject::connect(&editor, &SketchEditorDialog::sketchChanged, [&] { ++changes; });
    enterRadius(rounding, 10);
    ASSERT_TRUE(editor.sketch().arc(1));
    enterRadius(radius, 20);
    EXPECT_NEAR(editor.sketch().arc(1)->radiusMm, 20, 1e-9);
    EXPECT_EQ(changes, 2);
    QTemporaryDir dir;
    ASSERT_TRUE(editor.writeSketchFile(dir.filePath("rounding.json")));
    SketchEditorDialog restored;
    ASSERT_TRUE(restored.readSketchFile(dir.filePath("rounding.json")));
    expectShape(restored.sketch(), editor.sketch());
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
    auto* nozzleButton = editor.findChild<QPushButton*>("vortia-sketch-nozzle");
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
    ASSERT_TRUE(WorkspaceTestAccess::open(window, QString::fromStdString(root.string()) + "/"));
    EXPECT_EQ(WorkspaceTestAccess::title(window, 1), QStringLiteral("project"));
    const NozzleDimensions dims{90, 40, 140, 160, 460};
    auto nozzle = makeNozzleProfile(dims).value();
    ASSERT_TRUE(nozzle.filletVertex(0, 2));
    WorkspaceTestAccess::edit(window, nozzle, dims);
    WorkspaceTestAccess::setEndTime(window, 2.5);
    EXPECT_TRUE(WorkspaceTestAccess::dirty(window));
    WorkspaceTestAccess::add(window, "other");
    auto rectangle = makeRectangleSketch(55, 25).value();
    ASSERT_TRUE(rectangle.filletVertex(1, 5));
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

TEST(Workspace, ResultActorsAndSamplingNeverLeakIntoAnotherProject) {
    QTemporaryDir folder;ASSERT_TRUE(folder.isValid());
    vtkNew<vtkImageData> grid;grid->SetDimensions(2,2,2);
    vtkNew<vtkDoubleArray> field;field->SetName("p");field->SetNumberOfTuples(8);field->Fill(101325);
    grid->GetPointData()->AddArray(field);
    vtkNew<vtkAppendFilter> volume;volume->AddInputData(grid);volume->Update();
    vtkNew<vtkMultiBlockDataSet> blocks;blocks->SetBlock(0,volume->GetOutput());
    vtkNew<vtkXMLMultiBlockDataWriter> writer;
    const auto path=folder.filePath("fluid.vtm");writer->SetFileName(path.toStdString().c_str());writer->SetInputData(blocks);
    ASSERT_EQ(writer->Write(),1);
    MainWindow window;WorkspaceTestAccess::result(window,path);
    EXPECT_NO_THROW(WorkspaceTestAccess::bounds(window));
    ASSERT_EQ(WorkspaceTestAccess::actors(window),1);
    auto* selector=window.findChild<QComboBox*>("vortia-field-selector");ASSERT_NE(selector,nullptr);
    ASSERT_EQ(selector->count(),1);EXPECT_EQ(selector->currentData().toString(),"p");EXPECT_TRUE(selector->isEnabled());
    auto* section=window.findChild<QAction*>("vortia-plane-section-action");ASSERT_NE(section,nullptr);EXPECT_TRUE(section->isEnabled());
    for(auto* button:window.findChildren<QToolButton*>("vortia-ribbon-button"))if(button->defaultAction()&&button->defaultAction()->text()==MainWindow::tr("Sections")){EXPECT_TRUE(button->isEnabled());EXPECT_NE(button->menu(),nullptr);}
    WorkspaceTestAccess::lock(window,true);EXPECT_FALSE(section->isEnabled());WorkspaceTestAccess::lock(window,false);EXPECT_TRUE(section->isEnabled());
    WorkspaceTestAccess::add(window,"Empty project");EXPECT_EQ(selector->count(),0);EXPECT_FALSE(section->isEnabled());
    EXPECT_EQ(WorkspaceTestAccess::actors(window),1); // Only this project's domain outline.
    EXPECT_THROW(WorkspaceTestAccess::bounds(window),std::runtime_error);
    WorkspaceTestAccess::select(window,0);
    EXPECT_NO_THROW(WorkspaceTestAccess::bounds(window));
    EXPECT_EQ(WorkspaceTestAccess::actors(window),1);
    WorkspaceTestAccess::select(window,1);
    EXPECT_EQ(WorkspaceTestAccess::actors(window),1);
    EXPECT_THROW(WorkspaceTestAccess::bounds(window),std::runtime_error);
}

TEST(WorkspaceFlow, GeneralSettingsPersistAndRemainSeparateBetweenTabs) {
    QTemporaryDir folder;ASSERT_TRUE(folder.isValid());
    auto project=Project::create((folder.path()+"/editable").toStdString(),"editable");
    ASSERT_TRUE(project);
    MainWindow window;
    project->setSetupJson(WorkspaceTestAccess::setupJson(window));
    ASSERT_TRUE(project->save());
    ASSERT_TRUE(WorkspaceTestAccess::open(window,QString::fromStdString(project->rootDir().string())));
    const QJsonObject values{{"material",QJsonObject{{"Cp",1501},{"molecularWeight",24}}},
        {"boundaries",QJsonArray{}},{"mesh",QJsonObject{{"sizeMm",25}}}};
    WorkspaceTestAccess::flow(window,values);ASSERT_TRUE(WorkspaceTestAccess::save(window));
    WorkspaceTestAccess::add(window,"Another flow");EXPECT_TRUE(WorkspaceTestAccess::flow(window).isEmpty());
    WorkspaceTestAccess::select(window,1);EXPECT_EQ(WorkspaceTestAccess::flow(window),values);
    MainWindow reopened;
    ASSERT_TRUE(WorkspaceTestAccess::open(reopened,QString::fromStdString(project->rootDir().string())));
    EXPECT_EQ(WorkspaceTestAccess::flow(reopened),values);
    const auto actions=reopened.findChildren<QAction*>();
    EXPECT_TRUE(std::none_of(actions.cbegin(),actions.cend(),[](const QAction* a){return a->text().contains("Nozzle practice");}));
}

TEST(WorkspaceFlow, RuntimeEditsKeepTheCaseAndMigrateLegacyFingerprints) {
    QTemporaryDir folder;ASSERT_TRUE(folder.isValid());
    auto project=Project::create((folder.path()+"/editable").toStdString(),"editable");ASSERT_TRUE(project);
    MainWindow window;project->setSetupJson(WorkspaceTestAccess::setupJson(window));ASSERT_TRUE(project->save());
    ASSERT_TRUE(WorkspaceTestAccess::open(window,QString::fromStdString(project->rootDir().string())));
    auto shape=makeRectangleSketch(200,50).value();WorkspaceTestAccess::edit(window,shape,{});
    ASSERT_TRUE(WorkspaceTestAccess::save(window));
    QTimer::singleShot(0,[](){qobject_cast<QDialog*>(QApplication::activeModalWidget())->accept();});
    auto cfg=editFlowSetup(nullptr,shape,{});cfg.insert("activeCase","/saved/existing/case");
    WorkspaceTestAccess::flow(window,cfg);
    cfg.insert("preparedHash",WorkspaceTestAccess::fingerprint(window,true));WorkspaceTestAccess::flow(window,cfg);
    const auto expected=WorkspaceTestAccess::fingerprint(window);
    QTimer::singleShot(0,[](){
        auto* d=qobject_cast<QDialog*>(QApplication::activeModalWidget());ASSERT_NE(d,nullptr);
        d->findChild<QDoubleSpinBox*>("flow-solver-iterations")->setValue(5000);
        d->findChild<QDoubleSpinBox*>("flow-solver-processes")->setValue(2);
        d->findChild<QDoubleSpinBox*>("flow-convergence-fieldTolerance")->setValue(.0005);d->accept();
    });
    WorkspaceTestAccess::configure(window);
    EXPECT_EQ(WorkspaceTestAccess::flow(window).value("activeCase").toString(),"/saved/existing/case");
    EXPECT_EQ(WorkspaceTestAccess::flow(window).value("preparedHash").toString(),expected);
    EXPECT_EQ(WorkspaceTestAccess::fingerprint(window),expected);
    EXPECT_EQ(WorkspaceTestAccess::flow(window).value("solver").toObject().value("iterations").toInt(),5000);
    QTimer::singleShot(0,[](){auto* d=qobject_cast<QDialog*>(QApplication::activeModalWidget());d->findChild<QDoubleSpinBox*>("flow-material-Cp")->setValue(1501);d->accept();});
    WorkspaceTestAccess::configure(window);
    EXPECT_NE(WorkspaceTestAccess::fingerprint(window),expected);
    WorkspaceTestAccess::edit(window,makeRectangleSketch(300,50).value(),{});
    EXPECT_TRUE(WorkspaceTestAccess::flow(window).value("preparedHash").toString().isEmpty());
}

TEST(WorkspaceFlow, FailedControlUpdateNeverChangesTheActiveCase) {
    QTemporaryDir folder;ASSERT_TRUE(folder.isValid());
    ASSERT_TRUE(QDir().mkpath(folder.filePath("system")));ASSERT_TRUE(QDir().mkpath(folder.filePath("100")));
    writeFile(folder.filePath(".vortia-flow-case"),"vortia-axisymmetric-flow-v1\n");
    const QByteArray original="endTime 100; writeInterval 100; maxCo 0.05;\n";
    writeFile(folder.filePath("system/controlDict"),original);writeFile(folder.filePath("100/p"),"saved solution");
    const auto tool=folder.filePath("foamDictionary");
    writeFile(tool,"#!/bin/sh\necho damaged > \"$1\"\nexit 7\n");
    ASSERT_TRUE(QFile::setPermissions(tool,QFileDevice::ReadOwner|QFileDevice::WriteOwner|QFileDevice::ExeOwner));
    const QJsonObject cfg{{"iterations",200},{"writeInterval",100},{"maxCo",.1}};
    EXPECT_FALSE(updateFlowRunControls(folder.path(),cfg,{{"PATH",folder.path().toStdString()}}).isEmpty());
    EXPECT_EQ(readFile(folder.filePath("system/controlDict")),original);
    EXPECT_EQ(readFile(folder.filePath("100/p")),"saved solution");
    EXPECT_FALSE(updateFlowRunControls(folder.path(),QJsonObject{{"iterations",-1}},{}).isEmpty());
}

TEST(WorkspaceFlow, BundledBackendRevisionPreservesLegacySourcesAndRejectsModifiedCache) {
    QTemporaryDir folder;ASSERT_TRUE(folder.isValid());
    QDir().mkpath(folder.filePath("solver"));QDir().mkpath(folder.filePath("bin"));QDir().mkpath(folder.filePath("100"));
    writeFile(folder.filePath(".vortia-flow-case"),"vortia-axisymmetric-flow-v1\n");
    writeFile(folder.filePath("solver/vortiaCentralFoam.C"),"old source");
    writeFile(folder.filePath("bin/vortiaCentralFoam"),"old binary");
    writeFile(folder.filePath("100/p"),"saved pressure");
    ASSERT_TRUE(stageFlowBackend(folder.path()).isEmpty());
    const auto revision=readFile(folder.filePath(".vortia-solver-revision")).trimmed();
    ASSERT_EQ(revision.size(),64);
    ASSERT_TRUE(stageFlowBackend(folder.path()).isEmpty());
    EXPECT_EQ(readFile(folder.filePath("solver/vortiaCentralFoam.C")),"old source");
    EXPECT_EQ(readFile(folder.filePath("bin/vortiaCentralFoam")),"old binary");
    EXPECT_EQ(readFile(folder.filePath("100/p")),"saved pressure");
    writeFile(folder.filePath("solver-backends/"+QString::fromUtf8(revision)+"/vortiaCentralFoam.C"),"modified cache");
    EXPECT_FALSE(stageFlowBackend(folder.path()).isEmpty());
}

TEST(FlowRunControls, NativeDictionaryPreservesSavedFieldsWhenExplicitlyRequested) {
    const auto root=qEnvironmentVariable("VORTIA_FLOW_CONTROL_CASE");
    if(root.isEmpty())GTEST_SKIP()<<"Requires an explicit isolated native OpenFOAM case";
    const auto mesh=readFile(root+"/constant/polyMesh/boundary");const auto pressure=readFile(root+"/100/p");
    PosixProcessRunner processes;
    auto installation=openfoam::Detector::probe(qEnvironmentVariable("VORTIA_NATIVE_OF_ROOT").toStdString());ASSERT_TRUE(installation);
    auto captured=openfoam::Detector::captureEnvironment(*installation,processes);ASSERT_TRUE(captured);
    const auto& environment=*captured;
    QElapsedTimer clock;clock.start();
    EXPECT_TRUE(updateFlowRunControls(root,QJsonObject{{"iterations",200},{"writeInterval",10},{"maxCo",.05}},environment).isEmpty());
    std::cout<<"Native control update milliseconds="<<static_cast<double>(clock.nsecsElapsed())/1e6<<std::endl;
    EXPECT_EQ(readFile(root+"/constant/polyMesh/boundary"),mesh);EXPECT_EQ(readFile(root+"/100/p"),pressure);
    QProcessEnvironment native;
    for(const auto& [key,value]:environment)native.insert(QString::fromStdString(key),QString::fromStdString(value));
    QProcess check;check.setProcessEnvironment(native);
    const auto executable=openfoam::Detector::resolveExecutable("foamDictionary",environment);ASSERT_TRUE(executable);
    check.start(QString::fromStdString(executable->string()),{root+"/system/controlDict","-entry","endTime","-value"});
    ASSERT_TRUE(check.waitForFinished(10000));ASSERT_EQ(check.exitCode(),0);
    EXPECT_EQ(check.readAllStandardOutput().trimmed(),"200");
    const auto text=QString::fromUtf8(readFile(root+"/system/controlDict"));
    EXPECT_TRUE(text.contains(QRegularExpression("mass_inlet\\s*\\{[^}]*writeInterval\\s+100\\s*;")));
}

TEST(FlowRunControls, NativeContinuationStartsAfterTheSavedIterationWhenExplicitlyRequested) {
    const auto root=qEnvironmentVariable("VORTIA_FLOW_RESUME_CASE");
    if(root.isEmpty())GTEST_SKIP()<<"Requires an explicit isolated native OpenFOAM continuation case";
    PosixProcessRunner processes;
    auto installation=openfoam::Detector::probe(qEnvironmentVariable("VORTIA_NATIVE_OF_ROOT").toStdString());ASSERT_TRUE(installation);
    auto environment=openfoam::Detector::captureEnvironment(*installation,processes);ASSERT_TRUE(environment);
    const auto seed=readFile(root+"/100/p");const auto mesh=readFile(root+"/constant/polyMesh/boundary");
    ASSERT_TRUE(updateFlowRunControls(root,QJsonObject{{"iterations",200},{"writeInterval",100},{"maxCo",.05}},*environment).isEmpty());
    openfoam::CaseRunner runner(processes,*environment);
    const auto caps=openfoam::Detector::verifyCapabilities(*installation,*environment);
    double first=0;openfoam::RunCallbacks callbacks;
    callbacks.onProgress=[&](const openfoam::SolverProgress& progress){if(first==0&&progress.time>0)first=progress.time;};
    QElapsedTimer clock;clock.start();
    const auto outcome=runner.runSolver(root.toStdString(),{},caps,callbacks,nullptr,"vortiaCentralFoam");
    ASSERT_TRUE(outcome);ASSERT_TRUE(outcome->process.succeeded());EXPECT_FALSE(outcome->diverged);
    EXPECT_GT(first,100);EXPECT_DOUBLE_EQ(outcome->finalTime,200);
    EXPECT_EQ(readFile(root+"/100/p"),seed);EXPECT_EQ(readFile(root+"/constant/polyMesh/boundary"),mesh);
    EXPECT_TRUE(QFileInfo::exists(root+"/200/p"));
    std::cout<<"Native continuation: first="<<first<<" final="<<outcome->finalTime<<" milliseconds="<<static_cast<double>(clock.nsecsElapsed())/1e6<<std::endl;
}

TEST(ConsoleOutput, BatchesChunksInOrderAndClearCancelsPendingText) {
    ConsolePanel console;
    console.appendSolverOutput("Time = 1\n");console.appendSolverOutput("p = 10000\n");
    EXPECT_TRUE(console.toPlainText().isEmpty());console.flushOutput();
    EXPECT_EQ(console.toPlainText(),"Time = 1\np = 10000\n");
    console.appendSolverOutput("old project\n");console.clearConsole();console.flushOutput();
    EXPECT_TRUE(console.toPlainText().isEmpty());
    if(qEnvironmentVariableIsSet("VORTIA_CONSOLE_BENCHMARK")) {
        const QString chunk="Time = 100\nSolving for p, Initial residual = 0.1, Final residual = 0.01\n";
        QPlainTextEdit immediate;immediate.setMaximumBlockCount(20000);
        QElapsedTimer clock;clock.start();
        for(int i=0;i<10000;++i){immediate.moveCursor(QTextCursor::End);immediate.insertPlainText(chunk);immediate.ensureCursorVisible();}
        const auto unbatched=clock.nsecsElapsed();clock.restart();
        for(int i=0;i<10000;++i)console.appendSolverOutput(chunk);
        console.flushOutput();const auto batched=clock.nsecsElapsed();
        EXPECT_EQ(console.toPlainText(),immediate.toPlainText());
        std::cout<<"Console benchmark: immediate_ms="<<static_cast<double>(unbatched)/1e6<<" batched_ms="<<static_cast<double>(batched)/1e6<<std::endl;
    }
}

TEST(FlowRunControls, NativeParallelCheckpointWhenExplicitlyRequested) {
    const auto root=qEnvironmentVariable("VORTIA_PARALLEL_TEST_CASE");
    if(root.isEmpty())GTEST_SKIP()<<"Requires an isolated native OpenFOAM case";
    const auto mesh=readFile(root+"/constant/polyMesh/boundary"),seed=readFile(root+"/1000/p");
    ASSERT_FALSE(seed.isEmpty());
    const auto installation=openfoam::Detector::probe(qEnvironmentVariable("VORTIA_NATIVE_OF_ROOT").toStdString());ASSERT_TRUE(installation);
    PosixProcessRunner processes;
    const auto environment=openfoam::Detector::captureEnvironment(*installation,processes);ASSERT_TRUE(environment);
    const bool cancel=qEnvironmentVariableIsSet("VORTIA_PARALLEL_TEST_CANCEL");
    ASSERT_TRUE(updateFlowRunControls(root,QJsonObject{{"iterations",2000},{"writeInterval",cancel?100:1000},{"maxCo",.05}},*environment).isEmpty());
    ASSERT_TRUE(stageFlowBackend(root).isEmpty());
    CancellationToken token;openfoam::RunCallbacks callbacks;double first=0;
    callbacks.onProgress=[&](const auto& progress) {
        if(first==0&&progress.time>1000)first=progress.time;
        if(cancel&&progress.time>=1120)token.cancel();
    };
    openfoam::CaseRunner runner(processes,*environment);
    QElapsedTimer clock;clock.start();
    const auto outcome=runner.runSolver(root.toStdString(),{},openfoam::capabilitiesFor(*installation),callbacks,&token,"vortiaCentralFoam",8);
    ASSERT_TRUE(outcome)<<outcome.error().describe();
    EXPECT_EQ(readFile(root+"/1000/p"),seed);EXPECT_EQ(readFile(root+"/constant/polyMesh/boundary"),mesh);
    if(cancel) {
        EXPECT_TRUE(outcome->process.cancelled);EXPECT_TRUE(QFileInfo::exists(root+"/1100/p"));
    } else {
        EXPECT_TRUE(outcome->process.succeeded())<<outcome->process.stdErr;
        EXPECT_DOUBLE_EQ(outcome->finalTime,2000);EXPECT_TRUE(QFileInfo::exists(root+"/2000/p"));
    }
    EXPECT_GT(first,1000);
    std::cout<<"Native parallel orchestrated milliseconds="<<clock.elapsed()<<" cancel="<<cancel<<std::endl;
}

TEST(WorkspaceFlow, GeneralDialogReadsAndReturnsTheChosenGasAndSegmentAssignments) {
    Sketch2D shape;ASSERT_TRUE(shape.addPoint({0,0}));ASSERT_TRUE(shape.addPoint({200,0}));
    ASSERT_TRUE(shape.addPoint({200,50}));ASSERT_TRUE(shape.addPoint({0,50}));ASSERT_TRUE(shape.close());
    QTimer::singleShot(0,[]() {
        auto* dialog=qobject_cast<QDialog*>(QApplication::activeModalWidget());ASSERT_NE(dialog,nullptr);
        auto* cp=dialog->findChild<QDoubleSpinBox*>("flow-material-Cp");ASSERT_NE(cp,nullptr);cp->setValue(1501);
        auto* mw=dialog->findChild<QDoubleSpinBox*>("flow-material-molecularWeight");ASSERT_NE(mw,nullptr);mw->setValue(24);
        dialog->accept();
    });
    const auto config=editFlowSetup(nullptr,shape,{});
    EXPECT_DOUBLE_EQ(config.value("material").toObject().value("Cp").toDouble(),1501);
    EXPECT_DOUBLE_EQ(config.value("material").toObject().value("molecularWeight").toDouble(),24);
    ASSERT_EQ(config.value("boundaries").toArray().size(),4);
    EXPECT_EQ(config.value("boundaries").toArray()[0].toObject().value("type").toString(),"axis");
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
    RunMonitor monitor;ASSERT_NE(monitor.findChild<QWidget*>("monitor-detail-plot"),nullptr);EXPECT_TRUE(monitor.findChild<QWidget*>("monitor-detail-plot")->toolTip().contains(QString::fromUtf8("Pellizca")));
    EXPECT_EQ(SketchEditorDialog::tr("Nozzle dimensions"), QStringLiteral("Cotas de la tobera"));
    QDialogButtonBox buttons(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    EXPECT_EQ(buttons.button(QDialogButtonBox::Ok)->text(), QStringLiteral("Aceptar"));
    EXPECT_EQ(buttons.button(QDialogButtonBox::Cancel)->text(), QStringLiteral("Cancelar"));
    QMessageBox confirm(QMessageBox::Question, "Confirm", "Test", QMessageBox::Yes | QMessageBox::No);
    EXPECT_EQ(confirm.button(QMessageBox::Yes)->text().remove('&'), QStringLiteral("Sí"));
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

TEST(WorkspaceFlow, ModernSetupKeepsAutomaticCriteriaAndPageNavigation) {
    QTimer::singleShot(0,[](){
        auto* d=qobject_cast<QDialog*>(QApplication::activeModalWidget());ASSERT_NE(d,nullptr);
        auto* nav=d->findChild<QButtonGroup*>("flow-navigation");ASSERT_NE(nav,nullptr);EXPECT_EQ(nav->buttons().size(),4);
        auto* pages=d->findChild<QStackedWidget*>("flow-pages");ASSERT_NE(pages,nullptr);EXPECT_EQ(pages->currentIndex(),3);
        auto* enabled=d->findChild<QCheckBox*>("flow-convergence-enabled");ASSERT_NE(enabled,nullptr);EXPECT_TRUE(enabled->isChecked());
        d->findChild<QDoubleSpinBox*>("flow-convergence-stableSamples")->setValue(7);d->accept();
    });
    const auto shape=makeRectangleSketch(300,50).value();
    const auto cfg=editFlowSetup(nullptr,shape,{},3);
    const auto c=cfg.value("solver").toObject().value("convergence").toObject();
    EXPECT_TRUE(c.value("enabled").toBool());EXPECT_EQ(c.value("stableSamples").toInt(),7);
    EXPECT_EQ(c.value("interval").toInt(),50);
    QTimer::singleShot(0,[](){
        auto* d=qobject_cast<QDialog*>(QApplication::activeModalWidget());
        EXPECT_EQ(d->findChild<QDoubleSpinBox*>("flow-convergence-stableSamples")->value(),7);
        d->reject();
    });
    EXPECT_TRUE(editFlowSetup(nullptr,shape,cfg).isEmpty());
}

TEST(FlowRunControls, NativeConvergencePipelineWhenExplicitlyRequested) {
    const auto root=qEnvironmentVariable("VORTIA_CONVERGENCE_PIPELINE_CASE");
    if(root.isEmpty())GTEST_SKIP()<<"Requires an explicit private equilibrium native case";
    PosixProcessRunner processes;
    auto installation=openfoam::Detector::probe(qEnvironmentVariable("VORTIA_NATIVE_OF_ROOT").toStdString());ASSERT_TRUE(installation);
    auto env=openfoam::Detector::captureEnvironment(*installation,processes);ASSERT_TRUE(env);
    const auto before=readFile(root+"/0/p");
    ASSERT_TRUE(stageFlowBackend(root).isEmpty());
    ASSERT_TRUE(updateFlowRunControls(root,{{"iterations",1000},{"writeInterval",1000},{"maxCo",.05}},*env).isEmpty());
    openfoam::CaseRunner runner(processes,*env);const auto caps=openfoam::Detector::verifyCapabilities(*installation,*env);
    std::optional<openfoam::ConvergenceSample> last;openfoam::RunCallbacks callbacks;
    std::vector<openfoam::MonitorSample> monitors;callbacks.onMonitors=[&](const auto& samples){monitors.insert(monitors.end(),samples.begin(),samples.end());};
    callbacks.onProgress=[&](const openfoam::SolverProgress& p){if(p.convergence)last=p.convergence;};
    const auto mesh=runner.runMeshing(root.toStdString(),caps,{},nullptr);ASSERT_TRUE(mesh);ASSERT_TRUE(mesh->process.succeeded());
    const auto result=runner.runSolver(root.toStdString(),{},caps,callbacks,nullptr,"vortiaCentralFoam",4);
    ASSERT_TRUE(result);ASSERT_TRUE(result->process.succeeded());EXPECT_EQ(result->finalTime,700);
    ASSERT_TRUE(last);EXPECT_EQ(last->status,"converged");EXPECT_TRUE(last->massIncoming);EXPECT_TRUE(last->massOutgoing);
    EXPECT_TRUE(std::any_of(monitors.begin(),monitors.end(),[](const auto& sample){return sample.series=="flow.incoming"&&sample.coordinate==700;}));
    const auto bytes=readFile(root+"/vortia-convergence.json");
    const auto assessment=openfoam::parseConvergenceReport(bytes.toStdString());
    ASSERT_TRUE(assessment);EXPECT_EQ(assessment->status,"converged");EXPECT_EQ(assessment->iteration,700);
    EXPECT_TRUE(QFileInfo::exists(root+"/700/p"));EXPECT_EQ(readFile(root+"/0/p"),before);
}

TEST(ApplicationPreferences, RoundTripAndInvalidValuesUseSafeDefaults) {
    QTemporaryDir dir;QSettings settings(dir.filePath("preferences.ini"),QSettings::IniFormat);
    auto p=ApplicationPreferences::load(settings);EXPECT_TRUE(p.snapToGrid);EXPECT_EQ(p.checkpointInterval,500);
    p.cfdZoomSensitivity=1.8;p.cfdPanSensitivity=.7;p.defaultProcesses=3;p.snapSpacing=.25;p.zoomSensitivity=1.7;p.automaticPreview=false;p.density="compact";
    p.save(settings);auto loaded=ApplicationPreferences::load(settings);
    EXPECT_DOUBLE_EQ(loaded.cfdZoomSensitivity,1.8);EXPECT_DOUBLE_EQ(loaded.cfdPanSensitivity,.7);EXPECT_EQ(loaded.defaultProcesses,3);EXPECT_DOUBLE_EQ(loaded.snapSpacing,.25);EXPECT_FALSE(loaded.automaticPreview);
    EXPECT_EQ(loaded.flowDefaults().value("solver").toObject().value("processes").toInt(),3);
    settings.setValue("viewport/zoomSensitivity",10);settings.setValue("viewport/panSensitivity",QString("nan"));settings.setValue("calculation/processes",9000);settings.setValue("sketch/zoomSensitivity",QString("nan"));
    settings.setValue("sketch/snapSpacing",-1);settings.setValue("ui/density","unknown");
    loaded=ApplicationPreferences::load(settings);EXPECT_DOUBLE_EQ(loaded.cfdZoomSensitivity,1);EXPECT_DOUBLE_EQ(loaded.cfdPanSensitivity,1);EXPECT_EQ(loaded.defaultProcesses,0);
    EXPECT_DOUBLE_EQ(loaded.zoomSensitivity,1);EXPECT_DOUBLE_EQ(loaded.snapSpacing,1);EXPECT_EQ(loaded.density,"comfortable");
}
TEST(ApplicationPreferences, DialogAppliesOnlyAcceptedValuesAndLocksEngineDuringRun) {
    QTimer::singleShot(0,[](){auto* d=qobject_cast<QDialog*>(QApplication::activeModalWidget());ASSERT_NE(d,nullptr);
        EXPECT_EQ(d->findChild<QButtonGroup*>("preferences-navigation")->buttons().size(),4);
        auto* navigation=d->findChild<QButtonGroup*>("preferences-navigation");
        navigation->button(2)->setChecked(true);
        EXPECT_EQ(d->findChild<QStackedWidget*>("preferences-pages")->currentIndex(),2);
        EXPECT_FALSE(d->findChild<QLineEdit*>("preference-backend")->isEnabled());
        d->findChild<QSpinBox*>("preference-processes")->setValue(2);
        d->findChild<QCheckBox*>("preference-preview")->setChecked(false);d->accept();
    });
    const ApplicationPreferences original;
    auto result=editApplicationPreferences(nullptr,original,true);ASSERT_TRUE(result);EXPECT_EQ(result->defaultProcesses,2);
    EXPECT_FALSE(result->automaticPreview);EXPECT_TRUE(original.automaticPreview);
    QTimer::singleShot(0,[](){qobject_cast<QDialog*>(QApplication::activeModalWidget())->reject();});
    EXPECT_FALSE(editApplicationPreferences(nullptr,*result,false));
}
TEST(WorkspaceFlow, InjectedDefaultsNeverReplaceExistingProjectControls) {
    const QJsonObject defaults{{"solver",QJsonObject{{"iterations",7000},{"writeInterval",500},{"processes",3}}}};
    QTimer::singleShot(0,[](){qobject_cast<QDialog*>(QApplication::activeModalWidget())->accept();});
    auto cfg=editFlowSetup(nullptr,makeRectangleSketch(300,50).value(),{},3,defaults);
    EXPECT_EQ(cfg.value("solver").toObject().value("iterations").toInt(),7000);
    EXPECT_EQ(cfg.value("solver").toObject().value("processes").toInt(),3);
    auto solver=cfg.value("solver").toObject();solver.insert("iterations",2000);solver.insert("processes",1);cfg.insert("solver",solver);
    QTimer::singleShot(0,[](){qobject_cast<QDialog*>(QApplication::activeModalWidget())->accept();});
    const auto edited=editFlowSetup(nullptr,makeRectangleSketch(300,50).value(),cfg,3,defaults);
    EXPECT_EQ(edited.value("solver").toObject().value("iterations").toInt(),2000);
    EXPECT_EQ(edited.value("solver").toObject().value("processes").toInt(),1);
}
TEST(WorkspaceFlow, InviscidModelDisablesUnusedTransportInputsAndPersists) {
    QTimer::singleShot(0,[](){auto* d=qobject_cast<QDialog*>(QApplication::activeModalWidget());
        auto* model=d->findChild<QComboBox*>("flow-turbulence");ASSERT_NE(model,nullptr);
        model->setCurrentIndex(model->findData("inviscid"));
        EXPECT_FALSE(d->findChild<QDoubleSpinBox*>("flow-material-mu")->isEnabled());
        EXPECT_FALSE(d->findChild<QDoubleSpinBox*>("flow-material-conductivity")->isEnabled());d->accept();
    });
    auto cfg=editFlowSetup(nullptr,makeRectangleSketch(300,50).value(),{},1);
    EXPECT_EQ(cfg.value("material").toObject().value("turbulence").toString(),"inviscid");
    EXPECT_GT(cfg.value("material").toObject().value("mu").toDouble(),0);
}
TEST(Workspace, WorkflowRibbonContainsEachToolOnlyOnceAndKeepsFileTools) {
    MainWindow window;auto* ribbon=window.findChild<QTabWidget*>("vortia-ribbon");ASSERT_NE(ribbon,nullptr);
    ASSERT_EQ(ribbon->count(),5);EXPECT_EQ(ribbon->tabText(1),MainWindow::tr("Geometry"));
    EXPECT_EQ(ribbon->tabText(2),MainWindow::tr("Physics"));EXPECT_EQ(ribbon->tabText(3),MainWindow::tr("Calculation"));
    std::map<QAction*,int> counts;
    for(auto* button:ribbon->findChildren<QToolButton*>("vortia-ribbon-button"))++counts[button->defaultAction()];
    for(auto [action,count]:counts){ASSERT_NE(action,nullptr);EXPECT_EQ(count,1)<<action->text().toStdString();}
    EXPECT_NE(window.findChild<QAction*>("vortia-view-latest-result"),nullptr);
    auto* save=window.findChild<QPushButton*>("vortia-sketch-save");ASSERT_NE(save,nullptr);EXPECT_TRUE(save->isHidden());
}

TEST(Workspace, ToolSearchFiltersPublishedCommandsWithoutDuplicates) {
    MainWindow window;auto* action=window.findChild<QAction*>("vortia-find-tool");ASSERT_NE(action,nullptr);
    QTimer::singleShot(0,[](){auto* d=qobject_cast<QDialog*>(QApplication::activeModalWidget());ASSERT_NE(d,nullptr);
        auto* query=d->findChild<QLineEdit*>("command-query");auto* list=d->findChild<QListWidget*>("command-results");
        ASSERT_NE(query,nullptr);ASSERT_NE(list,nullptr);EXPECT_GT(list->count(),10);
        query->setText(MainWindow::tr("Preferences…"));EXPECT_EQ(list->count(),1);d->reject();
    });
    action->trigger();
}

TEST(ResultFiles, OldVisualizationCannotMasqueradeAsLatestNativeCheckpoint) {
    QTemporaryDir dir;QDir().mkpath(dir.filePath("100"));QDir().mkpath(dir.filePath("VTK"));
    writeFile(dir.filePath("100/p"),"checkpoint");writeFile(dir.filePath("VTK/flow_5.vtm"),"vtk");
    writeFile(dir.filePath("VTK/flow.vtm.series"),R"({"files":[{"name":"flow_5.vtm","time":100}]})");
    EXPECT_FALSE(currentVtmFile(dir.path()).isEmpty());
    QDir().mkpath(dir.filePath("200"));writeFile(dir.filePath("200/p"),"newer checkpoint");
    EXPECT_TRUE(currentVtmFile(dir.path()).isEmpty());
    EXPECT_FALSE(latestVtmFile(dir.filePath("VTK")).isEmpty()); // old snapshot still available explicitly
    writeFile(dir.filePath("VTK/flow_6.vtm"),"new vtk");
    writeFile(dir.filePath("VTK/flow.vtm.series"),R"({"files":[{"name":"flow_5.vtm","time":100},{"name":"flow_6.vtm","time":200}]})");
    EXPECT_TRUE(currentVtmFile(dir.path()).endsWith("flow_6.vtm"));
}

TEST(FlowParallelism, AutomaticSelectionAvoidsSmallMeshOversubscription) {
    EXPECT_EQ(recommendedFlowProcesses(0,8),1u);EXPECT_EQ(recommendedFlowProcesses(500,8),1u);
    EXPECT_EQ(recommendedFlowProcesses(12580,8),4u);EXPECT_EQ(recommendedFlowProcesses(33786,8),8u);
    EXPECT_EQ(recommendedFlowProcesses(100000,2),2u);EXPECT_EQ(recommendedFlowProcesses(100000,0),1u);
    const auto defaults=ApplicationPreferences{}.flowDefaults().value("solver").toObject();
    EXPECT_EQ(defaults.value("processes").toInt(-1),0);
}

TEST(CfdNavigation, PinchPanAndDragModesMoveOnlyTheCamera) {
    VtkViewport viewport;viewport.resize(800,600);viewport.show();QApplication::processEvents();
    vtkNew<vtkImageData> grid;grid->SetDimensions(3,3,3);
    vtkNew<vtkDoubleArray> field;field->SetName("p");field->SetNumberOfTuples(27);field->Fill(42);grid->GetPointData()->AddArray(field);
    viewport.showDataSet(grid,"p");viewport.setViewXY();
    auto initial=viewport.copyCamera();const QPointF center(344,300);
    QNativeGestureEvent pinch(Qt::ZoomNativeGesture,QPointingDevice::primaryPointingDevice(),2,center,center,center,.5,{});
    QApplication::sendEvent(&viewport,&pinch);auto zoomed=viewport.copyCamera();
    EXPECT_NEAR(zoomed->GetParallelScale(),initial->GetParallelScale()/1.5,1e-8);
    QWheelEvent swipe(center,center,QPoint(40,-20),{},Qt::NoButton,Qt::NoModifier,Qt::ScrollUpdate,false,Qt::MouseEventSynthesizedBySystem);
    QApplication::sendEvent(&viewport,&swipe);auto panned=viewport.copyCamera();
    EXPECT_DOUBLE_EQ(panned->GetParallelScale(),zoomed->GetParallelScale());
    EXPECT_NE(panned->GetFocalPoint()[0],zoomed->GetFocalPoint()[0]);
    for(int i=0;i<3;++i)EXPECT_NEAR(panned->GetDirectionOfProjection()[i],zoomed->GetDirectionOfProjection()[i],1e-10);
    const auto mouse=[&](QEvent::Type type,QPointF at,Qt::MouseButton button,Qt::MouseButtons buttons,Qt::KeyboardModifiers modifiers=Qt::NoModifier){QMouseEvent event(type,at,at,at,button,buttons,modifiers);QApplication::sendEvent(&viewport,&event);};
    viewport.setNavigationMode(VtkViewport::NavigationMode::Pan);
    mouse(QEvent::MouseButtonPress,center,Qt::LeftButton,Qt::LeftButton);mouse(QEvent::MouseMove,center+QPointF(25,10),Qt::NoButton,Qt::LeftButton);mouse(QEvent::MouseButtonRelease,center+QPointF(25,10),Qt::LeftButton,Qt::NoButton);
    auto dragged=viewport.copyCamera();EXPECT_NE(dragged->GetFocalPoint()[0],panned->GetFocalPoint()[0]);
    QEvent focusLost(QEvent::FocusOut);QApplication::sendEvent(&viewport,&focusLost);
    mouse(QEvent::MouseMove,center+QPointF(100,100),Qt::NoButton,Qt::LeftButton);
    auto disarmed=viewport.copyCamera();for(int i=0;i<3;++i)EXPECT_DOUBLE_EQ(disarmed->GetPosition()[i],dragged->GetPosition()[i]);
    viewport.setNavigationMode(VtkViewport::NavigationMode::Zoom);
    mouse(QEvent::MouseButtonPress,center,Qt::LeftButton,Qt::LeftButton);mouse(QEvent::MouseMove,center+QPointF(0,-20),Qt::NoButton,Qt::LeftButton);mouse(QEvent::MouseButtonRelease,center,Qt::LeftButton,Qt::NoButton);
    EXPECT_LT(viewport.copyCamera()->GetParallelScale(),dragged->GetParallelScale());
    EXPECT_EQ(grid->GetNumberOfPoints(),27);EXPECT_DOUBLE_EQ(field->GetValue(0),42);
}
TEST(FieldTools, CancellingSectionRestoresPreviousPlaneAndCamera) {
    VtkViewport viewport;vtkNew<vtkImageData> grid;grid->SetDimensions(3,3,3);
    vtkNew<vtkDoubleArray> field;field->SetName("p");field->SetNumberOfTuples(27);field->Fill(42);grid->GetPointData()->AddArray(field);
    viewport.showDataSet(grid,"p");viewport.setSlicePlane(0,.5);viewport.setViewYZ();auto original=viewport.copyCamera();
    QTimer::singleShot(0,[&](){auto* dialog=qobject_cast<QDialog*>(QApplication::activeModalWidget());ASSERT_NE(dialog,nullptr);
        auto* axis=dialog->findChild<QComboBox*>("section-axis");ASSERT_NE(axis,nullptr);axis->setCurrentIndex(1);
        QTimer::singleShot(100,dialog,[&,dialog](){EXPECT_TRUE(viewport.sliceEnabled());EXPECT_EQ(viewport.slicePlaneState().axis,1);dialog->reject();});
    });
    showPlaneSection(nullptr,&viewport);auto restored=viewport.copyCamera();
    EXPECT_TRUE(viewport.sliceEnabled());EXPECT_EQ(viewport.slicePlaneState().axis,0);EXPECT_DOUBLE_EQ(viewport.slicePlaneState().position,.5);
    for(int i=0;i<3;++i){EXPECT_DOUBLE_EQ(restored->GetPosition()[i],original->GetPosition()[i]);EXPECT_DOUBLE_EQ(restored->GetFocalPoint()[i],original->GetFocalPoint()[i]);}
    viewport.clearSlicePlane();EXPECT_FALSE(viewport.sliceEnabled());EXPECT_DOUBLE_EQ(viewport.probePoint({.5,.5,.5},"p").value,42);
}

TEST(Workspace, RibbonMenusExposeTheirCommandsAndFieldPopupContainsOnlyData) {
    MainWindow window;
    auto* ribbon=window.findChild<QTabWidget*>("vortia-ribbon");ASSERT_NE(ribbon,nullptr);ribbon->setCurrentIndex(4);
    QToolButton* files=nullptr;
    for(auto* button:ribbon->findChildren<QToolButton*>("vortia-ribbon-button")){
        ASSERT_NE(button->defaultAction(),nullptr);
        if(button->defaultAction()->text()==MainWindow::tr("Result files"))files=button;
        if(button->defaultAction()->menu())EXPECT_EQ(button->menu(),button->defaultAction()->menu());
    }
    ASSERT_NE(files,nullptr);ASSERT_NE(files->menu(),nullptr);ASSERT_EQ(files->menu()->actions().size(),2);
    window.show();QApplication::processEvents();
    bool menuOpened=false;
    QTimer::singleShot(0,[&](){menuOpened=files->menu()->isVisible();files->menu()->close();});
    files->showMenu();EXPECT_TRUE(menuOpened);
    auto* selector=window.findChild<QComboBox*>("vortia-field-selector");ASSERT_NE(selector,nullptr);
    EXPECT_FALSE(selector->isEnabled());
}

TEST(SketchEditing, KeyboardUndoDeleteAndRedoRestoreTheGeometry){
    SketchEditorDialog editor;Sketch2D shape;ASSERT_TRUE(shape.addPoint({0,0}));ASSERT_TRUE(shape.addPoint({10,0}));ASSERT_TRUE(shape.addPoint({10,10}));ASSERT_TRUE(shape.close());editor.setSketch(shape);
    auto* table=editor.findChild<QTableWidget*>();auto* canvas=editor.findChild<QWidget*>("vortia-sketch-canvas");ASSERT_TRUE(table&&canvas);
    emit table->cellClicked(1,0);QKeyEvent remove(QEvent::KeyPress,Qt::Key_Backspace,Qt::NoModifier);QApplication::sendEvent(canvas,&remove);EXPECT_EQ(editor.sketch().points().size(),2);EXPECT_FALSE(editor.sketch().closed());
    QKeyEvent undo(QEvent::KeyPress,Qt::Key_Z,Qt::ControlModifier);QApplication::sendEvent(canvas,&undo);expectShape(editor.sketch(),shape);
    editor.redoEdit();EXPECT_EQ(editor.sketch().points().size(),2);editor.undoEdit();
    editor.setReadOnly(true);emit table->cellClicked(1,0);editor.deleteSelectedPoint();editor.redoEdit();expectShape(editor.sketch(),shape);
}
TEST(SketchEditing, WholeSketchClipboardPreservesArcsAndRejectsMalformedGeometry){
    SketchEditorDialog source,destination;Sketch2D circle;for(const auto point:{SketchPoint2D{10,0},{0,10},{-10,0},{0,-10}})ASSERT_TRUE(circle.addPoint(point));ASSERT_TRUE(circle.close());for(std::size_t i=0;i<4;++i)ASSERT_TRUE(circle.setSegmentBulge(i,std::tan(std::numbers::pi/8)));source.setSketch(circle);source.copySelection();
    QTimer::singleShot(0,[](){auto* d=qobject_cast<QDialog*>(QApplication::activeModalWidget());ASSERT_NE(d,nullptr);EXPECT_EQ(d->objectName(),"vortia-paste-geometry");d->accept();});
    destination.pasteGeometry();expectShape(destination.sketch(),circle);destination.undoEdit();EXPECT_TRUE(destination.sketch().points().empty());destination.redoEdit();expectShape(destination.sketch(),circle);
    QApplication::clipboard()->setText("{invalid geometry}");destination.pasteGeometry();expectShape(destination.sketch(),circle);
    auto* x=destination.findChild<QDoubleSpinBox*>("vortia-sketch-x");ASSERT_NE(x,nullptr);auto* text=x->findChild<QLineEdit*>();ASSERT_NE(text,nullptr);text->setText("123");text->selectAll();QKeyEvent remove(QEvent::KeyPress,Qt::Key_Backspace,Qt::NoModifier);QApplication::sendEvent(text,&remove);expectShape(destination.sketch(),circle);
}
TEST(SketchEditing, UndoStacksStayWithTheirProjectTab){
    MainWindow window;auto& editor=WorkspaceTestAccess::editor(window);auto* add=editor.findChild<QPushButton*>("vortia-sketch-add");auto* x=editor.findChild<QDoubleSpinBox*>("vortia-sketch-x");ASSERT_TRUE(add&&x);x->setValue(1);add->click();EXPECT_EQ(editor.sketch().points().size(),1);
    WorkspaceTestAccess::add(window,"Second");x->setValue(2);add->click();WorkspaceTestAccess::select(window,0);editor.undoEdit();EXPECT_TRUE(editor.sketch().points().empty());WorkspaceTestAccess::select(window,1);ASSERT_EQ(editor.sketch().points().size(),1);EXPECT_DOUBLE_EQ(editor.sketch().points()[0].xMm,2);editor.undoEdit();EXPECT_TRUE(editor.sketch().points().empty());
}
TEST(SketchEditing, AVertexDragIsOneUndoableEdit){
    SketchEditorDialog editor;editor.resize(1000,700);Sketch2D shape;ASSERT_TRUE(shape.addPoint({50,0}));ASSERT_TRUE(shape.addPoint({100,20}));ASSERT_TRUE(shape.addPoint({150,0}));editor.setSketch(shape);editor.show();QApplication::processEvents();auto* canvas=editor.findChild<QWidget*>("vortia-sketch-canvas");ASSERT_NE(canvas,nullptr);
    const double scale=std::min((canvas->width()-65.0)/200.0,(canvas->height()-65.0)/140.0);const QPointF point(canvas->width()/2.0-50*scale,canvas->height()/2.0);
    const auto mouse=[&](QEvent::Type type,QPointF at,Qt::MouseButton button,Qt::MouseButtons buttons){QMouseEvent event(type,at,at,at,button,buttons,Qt::NoModifier);QApplication::sendEvent(canvas,&event);};
    mouse(QEvent::MouseButtonPress,point,Qt::LeftButton,Qt::LeftButton);mouse(QEvent::MouseMove,point+QPointF(10,10),Qt::NoButton,Qt::LeftButton);mouse(QEvent::MouseMove,point+QPointF(30,10),Qt::NoButton,Qt::LeftButton);mouse(QEvent::MouseButtonRelease,point+QPointF(30,10),Qt::LeftButton,Qt::NoButton);
    EXPECT_NE(editor.sketch().points()[0].xMm,50);EXPECT_EQ(editor.editHistory().undo.size(),1);editor.undoEdit();expectShape(editor.sketch(),shape);
}
TEST(RunHistory, BoundedDecimationRetainsExtremaAndExportsRawZeroValues){
    MonitorHistory history;std::vector<openfoam::MonitorSample> samples;for(int i=0;i<20000;++i)samples.push_back({static_cast<double>(i),i==10000?1000:i%2?1.0:0.0,"residual.initial.p",1});history.append(samples);
    ASSERT_EQ(history.series.size(),1);const auto& points=history.series.begin()->second;EXPECT_LE(points.size(),8192u);EXPECT_TRUE(history.reduced);EXPECT_EQ(history.observations,20000u);EXPECT_DOUBLE_EQ(points.front().x,0);EXPECT_DOUBLE_EQ(points.back().x,19999);EXPECT_TRUE(std::any_of(points.begin(),points.end(),[](const auto& p){return p.y==1000;}));
    QTemporaryDir folder;const auto file=folder.filePath("history.csv");ASSERT_TRUE(history.writeCsv(file));const auto text=readFile(file);EXPECT_TRUE(text.startsWith("session,solver_coordinate,series,value,unit"));EXPECT_TRUE(text.contains("1,0,residual.initial.p,0,1"));
}
TEST(RunHistory, CancellingAReadCannotPublishIntoTheNextCase){
    QTemporaryDir a,b;QByteArray large;for(int i=0;i<10000;++i)large+="Time = "+QByteArray::number(i)+"\nDICPCG: Solving for p, Initial residual = 0.2, Final residual = 0.01, No Iterations 1\n";
    writeFile(a.filePath("log.vortia"),large);writeFile(b.filePath("log.vortia"),"Time = 7\nDICPCG: Solving for Ux, Initial residual = 0.3, Final residual = 0.02, No Iterations 1\n");
    RunMonitor monitor;monitor.setCase(a.path());monitor.setCase(b.path());QElapsedTimer timer;timer.start();while(monitor.loading()&&timer.elapsed()<3000){QApplication::processEvents();QThread::msleep(1);}ASSERT_FALSE(monitor.loading());QApplication::processEvents();EXPECT_FALSE(monitor.history().series.contains("residual.initial.p"));ASSERT_TRUE(monitor.history().series.contains("residual.initial.Ux"));EXPECT_EQ(monitor.history().series.at("residual.initial.Ux").size(),1u);
}
TEST(RunHistory, ReplayAndLiveSamplesRemainBoundToTheirCaseAndFreezeDoesNotStopRecording){
    QTemporaryDir a,b;writeFile(a.filePath("log.vortia"),"Time = 10\nDICPCG: Solving for p, Initial residual = 0.2, Final residual = 0.01, No Iterations 1\n");
    RunMonitor monitor;monitor.setCase(a.path());QElapsedTimer timer;timer.start();while(monitor.loading()&&timer.elapsed()<3000){QApplication::processEvents();QThread::msleep(1);}ASSERT_FALSE(monitor.loading());ASSERT_EQ(monitor.history().series.at("residual.initial.p").size(),1u);
    monitor.beginRun(a.path());auto* pause=monitor.findChild<QCheckBox*>("monitor-pause");ASSERT_NE(pause,nullptr);pause->setChecked(true);monitor.appendBatch(R"([[20,0.1,"residual.initial.p",0],[20,0.005,"residual.final.p",0]])");EXPECT_EQ(monitor.history().series.at("residual.initial.p").size(),2u);
    monitor.setCase(b.path());EXPECT_TRUE(monitor.history().series.empty());monitor.appendBatch(R"([[30,1,"residual.initial.p",0]])");EXPECT_TRUE(monitor.history().series.empty());monitor.setCase(a.path());timer.restart();while(monitor.loading()&&timer.elapsed()<3000){QApplication::processEvents();QThread::msleep(1);}EXPECT_FALSE(monitor.history().series.empty());
}

namespace {
Sketch2D boundaryExample(){Sketch2D shape;for(const auto p:{SketchPoint2D{0,0},{0,10},{10,10},{20,5},{30,5},{30,0}})if(!shape.addPoint(p))return {};if(!shape.close())return {};return shape;}
QJsonObject assignedExample(){QJsonArray boundaries;for(const auto* kind:{"inlet","wall","wall","wall","outlet","axis"})boundaries.append(QJsonObject{{"name",kind},{"type",kind},{"pressure",QString(kind)=="inlet"?3640000:QString(kind)=="outlet"?10000:101325},{"temperature",QString(kind)=="inlet"?2900:300},{"viscosityRatio",10}});return {{"boundaries",boundaries}};}
}
TEST(BoundaryMapping, FilletsKeepExistingPhysicalAssignmentsAndUndoRestoresThem){
    const auto original=boundaryExample();auto edited=original;ASSERT_TRUE(edited.filletVertex(2,1));const auto before=assignedExample();const auto mapped=reconcileFlowBoundaries(original,edited,before);const auto boundaries=mapped.value("boundaries").toArray();ASSERT_EQ(boundaries.size(),7);
    EXPECT_EQ(boundaries[0],before.value("boundaries").toArray()[0]);EXPECT_EQ(boundaries[2].toObject().value("type").toString(),"wall");EXPECT_EQ(boundaries[5],before.value("boundaries").toArray()[4]);EXPECT_EQ(boundaries[6].toObject().value("type").toString(),"axis");
    const auto restored=reconcileFlowBoundaries(edited,original,mapped);EXPECT_EQ(restored.value("boundaries"),before.value("boundaries"));
    auto radius=edited;ASSERT_TRUE(radius.setFilletRadius(2,2));const auto changed=reconcileFlowBoundaries(edited,radius,mapped);EXPECT_EQ(changed.value("boundaries"),mapped.value("boundaries"));
}
TEST(BoundaryMapping, SplitInletsInheritValuesAndAmbiguousNewArcsStayUnassigned){
    const auto original=boundaryExample();auto split=original;ASSERT_TRUE(split.splitSegment(0));const auto cfg=assignedExample();const auto mapped=reconcileFlowBoundaries(original,split,cfg).value("boundaries").toArray();ASSERT_EQ(mapped.size(),7);EXPECT_EQ(mapped[0],cfg.value("boundaries").toArray()[0]);EXPECT_EQ(mapped[1],mapped[0]);
    auto mixed=original;ASSERT_TRUE(mixed.filletVertex(1,1));const auto ambiguous=reconcileFlowBoundaries(original,mixed,cfg);EXPECT_EQ(ambiguous.value("boundaries").toArray()[1].toObject().value("type").toString(),"unassigned");EXPECT_EQ(ambiguous.value("boundaries").toArray()[0],cfg.value("boundaries").toArray()[0]);
}
TEST(BoundaryMapping, DeletingAMixedCornerKeepsRecoverableAssignmentsAndGeometryUndo){
    const auto original=boundaryExample();auto removed=original;ASSERT_TRUE(removed.removePoint(1));const auto cfg=assignedExample();const auto mapped=reconcileFlowBoundaries(original,removed,cfg);EXPECT_EQ(mapped.value("boundaries").toArray()[0].toObject().value("type").toString(),"unassigned");EXPECT_FALSE(mapped.value("unmappedBoundaries").toArray().empty());EXPECT_EQ(reconcileFlowBoundaries(removed,original,mapped).value("boundaries"),cfg.value("boundaries"));
}
TEST(BoundaryMapping, RegionNamesAreRepairedWithoutChangingOpenBoundaryValues){
    auto cfg=assignedExample();auto boundaries=cfg.value("boundaries").toArray();auto inlet=boundaries[0].toObject();inlet.insert("name",QString::fromUtf8("Cámara de 3,64 MPa"));auto outlet=boundaries[4].toObject();outlet.insert("name",inlet.value("name"));boundaries[0]=inlet;boundaries[4]=outlet;const auto fixed=normalizedBoundaryNames(boundaries);
    EXPECT_EQ(fixed[0].toObject().value("name").toString(),"Camara_de_3_64_MPa");EXPECT_NE(fixed[4].toObject().value("name"),fixed[0].toObject().value("name"));EXPECT_EQ(fixed[0].toObject().value("pressure"),3640000);EXPECT_EQ(fixed[4].toObject().value("pressure"),10000);
}
TEST(SketchEditing, CoordinateTableEditsAreUndoableAndImpossibleMovesAreRetained){
    SketchEditorDialog editor;Sketch2D shape;ASSERT_TRUE(shape.addPoint({0,0}));ASSERT_TRUE(shape.addPoint({10,0}));ASSERT_TRUE(shape.addPoint({10,10}));editor.setSketch(shape);auto* table=editor.findChild<QTableWidget*>();ASSERT_NE(table,nullptr);table->item(1,1)->setData(Qt::EditRole,12.5);EXPECT_DOUBLE_EQ(editor.sketch().points()[1].xMm,12.5);QApplication::processEvents();editor.undoEdit();expectShape(editor.sketch(),shape);
    table->item(1,1)->setData(Qt::EditRole,0.0);EXPECT_DOUBLE_EQ(editor.sketch().points()[1].xMm,10);QApplication::processEvents();expectShape(editor.sketch(),shape);
}
TEST(SketchGeometry, FilletRadiusIntervalsContainOnlyFeasibleLimits){
    auto shape=makeRectangleSketch(3,2).value();const auto range=shape.filletRadiusRange(1);ASSERT_TRUE(range);EXPECT_LT(range->second,2);auto lower=shape,upper=shape;EXPECT_TRUE(lower.filletVertex(1,range->first));EXPECT_TRUE(upper.filletVertex(1,range->second));const auto arcRange=upper.arcFilletRadiusRange(1);ASSERT_TRUE(arcRange);EXPECT_TRUE(upper.setFilletRadius(1,arcRange->first));
}
TEST(FlowBoundaryEditor, DisplaysNumberedGeometryAndRetainsAllBoundaryRows){
    const auto original=boundaryExample();auto edited=original;ASSERT_TRUE(edited.filletVertex(2,1));auto cfg=assignedExample();cfg.insert("boundaryGeometry",boundaryGeometry(original));QTimer::singleShot(0,[](){auto* d=qobject_cast<QDialog*>(QApplication::activeModalWidget());ASSERT_NE(d,nullptr);EXPECT_NE(d->findChild<QWidget*>("flow-boundary-preview"),nullptr);auto* table=d->findChild<QTableWidget*>("flow-boundaries");ASSERT_NE(table,nullptr);EXPECT_EQ(table->rowCount(),7);EXPECT_EQ(static_cast<QComboBox*>(table->cellWidget(5,2))->currentData().toString(),"outlet");table->setCurrentCell(2,0);EXPECT_TRUE(d->findChild<QLabel*>("flow-boundary-details")->text().contains("3"));d->accept();});
    const auto result=editFlowSetup(nullptr,edited,cfg,2);EXPECT_EQ(result.value("boundaries").toArray()[0].toObject().value("pressure"),3640000);EXPECT_TRUE(result.contains("boundaryGeometry"));
}

TEST(FlowBoundaryEditor, NavigationSelectsTinyArcsWithoutChangingAssignments){
    const auto original=boundaryExample();auto shape=original;ASSERT_TRUE(shape.filletVertex(2,.005));auto cfg=reconcileFlowBoundaries(original,shape,assignedExample());
    QTimer::singleShot(0,[&](){
        auto* dialog=qobject_cast<QDialog*>(QApplication::activeModalWidget());ASSERT_NE(dialog,nullptr);auto* preview=dynamic_cast<BoundaryPreview*>(dialog->findChild<QWidget*>("flow-boundary-preview"));auto* table=dialog->findChild<QTableWidget*>("flow-boundaries");ASSERT_NE(preview,nullptr);ASSERT_NE(table,nullptr);QApplication::processEvents();
        const auto fitted=preview->viewTransform();const auto v=shape.segmentPoint(2,.5);const QPointF world(v.xMm,v.yMm),anchor=fitted.map(world);
        for(int i=0;i<7;++i){QNativeGestureEvent pinch(Qt::ZoomNativeGesture,QPointingDevice::primaryPointingDevice(),2,anchor,anchor,anchor,1,{});QApplication::sendEvent(preview,&pinch);}
        const auto zoomed=preview->viewTransform();EXPECT_NEAR(zoomed.m11(),fitted.m11()*128,1e-8);EXPECT_LT(QLineF(zoomed.map(world),anchor).length(),1e-8);
        QWheelEvent wheel(anchor,anchor,{},QPoint(0,120),Qt::NoButton,Qt::NoModifier,Qt::NoScrollPhase,false);QApplication::sendEvent(preview,&wheel);EXPECT_NEAR(preview->viewTransform().m11(),zoomed.m11()*1.2,1e-8);EXPECT_LT(QLineF(preview->viewTransform().map(world),anchor).length(),1e-8);
        QWheelEvent swipe(anchor,anchor,QPoint(30,-15),{},Qt::NoButton,Qt::NoModifier,Qt::ScrollUpdate,false);QApplication::sendEvent(preview,&swipe);EXPECT_LT(QLineF(preview->viewTransform().map(world),anchor+QPointF(30,-15)).length(),1e-8);
        const auto mouse=[&](QEvent::Type type,QPointF at,Qt::MouseButton button,Qt::MouseButtons buttons){QMouseEvent event(type,at,at,at,button,buttons,Qt::NoModifier);QApplication::sendEvent(preview,&event);};
        QPointF click=preview->viewTransform().map(world);mouse(QEvent::MouseButtonPress,click,Qt::LeftButton,Qt::LeftButton);mouse(QEvent::MouseButtonRelease,click,Qt::LeftButton,Qt::NoButton);EXPECT_EQ(table->currentRow(),2);
        mouse(QEvent::MouseButtonPress,click,Qt::LeftButton,Qt::LeftButton);mouse(QEvent::MouseMove,click+QPointF(45,20),Qt::NoButton,Qt::LeftButton);mouse(QEvent::MouseButtonRelease,click+QPointF(45,20),Qt::LeftButton,Qt::NoButton);EXPECT_EQ(table->currentRow(),2);EXPECT_LT(QLineF(preview->viewTransform().map(world),click+QPointF(45,20)).length(),1e-8);
        QKeyEvent fit(QEvent::KeyPress,Qt::Key_F,Qt::NoModifier);QApplication::sendEvent(preview,&fit);EXPECT_EQ(preview->viewTransform(),fitted);
        QNativeGestureEvent pinch(Qt::ZoomNativeGesture,QPointingDevice::primaryPointingDevice(),2,anchor,anchor,anchor,.5,{});QApplication::sendEvent(preview,&pinch);auto* fitButton=dialog->findChild<QToolButton*>("flow-boundary-fit");ASSERT_NE(fitButton,nullptr);fitButton->click();EXPECT_EQ(preview->viewTransform(),fitted);
        QWheelEvent zoomAgain(anchor,anchor,{},QPoint(0,120),Qt::NoButton,Qt::NoModifier,Qt::NoScrollPhase,false);QApplication::sendEvent(preview,&zoomAgain);mouse(QEvent::MouseButtonDblClick,anchor,Qt::LeftButton,Qt::LeftButton);EXPECT_EQ(preview->viewTransform(),fitted);
        dialog->accept();
    });
    const auto accepted=editFlowSetup(nullptr,shape,cfg,2);EXPECT_EQ(accepted.value("boundaries"),cfg.value("boundaries"));EXPECT_EQ(boundaryGeometry(shape),cfg.value("boundaryGeometry"));
}
TEST(RunHistory, LargeNativeHistoryReplayWhenExplicitlyRequested){
    const auto root=qEnvironmentVariable("VORTIA_HISTORY_REPLAY_CASE");if(root.isEmpty())GTEST_SKIP()<<"Read-only optional native log replay";
    RunMonitor monitor;QElapsedTimer timer;timer.start();monitor.setCase(root);while(monitor.loading()&&timer.elapsed()<30000){QApplication::processEvents();QThread::msleep(1);}ASSERT_FALSE(monitor.loading());EXPECT_GT(monitor.history().observations,100u);EXPECT_TRUE(monitor.history().series.contains("mass.global"));for(const auto& [key,points]:monitor.history().series)EXPECT_LE(points.size(),8192u);
    qInfo("Native history replay: %lld ms, %llu observations, %zu series",timer.elapsed(),static_cast<unsigned long long>(monitor.history().observations),monitor.history().series.size());
}
TEST(BoundaryMapping, NonPhysicalMetadataDoesNotInvalidateTheCalculationFingerprint){
    QTemporaryDir dir;auto project=Project::create(dir.filePath("editable").toStdString(),"editable");ASSERT_TRUE(project);MainWindow window;project->setSetupJson(WorkspaceTestAccess::setupJson(window));ASSERT_TRUE(project->save());ASSERT_TRUE(WorkspaceTestAccess::open(window,QString::fromStdString(project->rootDir().string())));
    auto cfg=assignedExample();WorkspaceTestAccess::flow(window,cfg);ASSERT_TRUE(WorkspaceTestAccess::save(window));const auto before=WorkspaceTestAccess::fingerprint(window);
    cfg.insert("boundaryGeometry",boundaryGeometry(boundaryExample()));cfg.insert("boundaryVersions",QJsonArray{QJsonObject{{"example",1}}});cfg.insert("unmappedBoundaries",QJsonArray{QJsonObject{{"name","old-wall"}}});WorkspaceTestAccess::flow(window,cfg);EXPECT_EQ(WorkspaceTestAccess::fingerprint(window),before);
}
TEST(BoundaryMapping, SafeSettingsRepairRetainsPhysicalValues){
    const auto shape=boundaryExample();auto cfg=assignedExample();cfg.insert("solver",QJsonObject{{"iterations",100},{"writeInterval",500},{"convergence",QJsonObject{{"interval",100},{"minimumIterations",50}}}});cfg.insert("mesh",QJsonObject{{"sizeMm",10},{"wallSizeMm",20}});const auto fixed=normalizedFlowConfiguration(shape,cfg);
    EXPECT_EQ(fixed.value("solver").toObject().value("writeInterval"),100);EXPECT_EQ(fixed.value("solver").toObject().value("convergence").toObject().value("minimumIterations"),100);EXPECT_EQ(fixed.value("mesh").toObject().value("wallSizeMm"),10);EXPECT_EQ(fixed.value("boundaries").toArray()[0].toObject().value("pressure"),3640000);
}

TEST(EngineeringUi, RenderComponentsWhenExplicitlyRequested){
    const auto output=qEnvironmentVariable("VORTIA_UI_ARTIFACT_DIR"),source=qEnvironmentVariable("VORTIA_HISTORY_REPLAY_CASE");if(output.isEmpty()||source.isEmpty())GTEST_SKIP()<<"Optional component render QA";
    QTranslator translator;ASSERT_TRUE(translator.load(QStringLiteral(VORTIA_TEST_QM)));QApplication::installTranslator(&translator);
    ASSERT_TRUE(QDir().mkpath(output));ThemeManager::apply(*qobject_cast<QApplication*>(QApplication::instance()),ThemeManager::Theme::Dark);
    RunMonitor monitor;ASSERT_NE(monitor.findChild<QWidget*>("monitor-detail-plot"),nullptr);EXPECT_TRUE(monitor.findChild<QWidget*>("monitor-detail-plot")->toolTip().contains(QString::fromUtf8("Pellizca")));monitor.resize(1200,740);monitor.show();monitor.setCase(source);QElapsedTimer timer;timer.start();while(monitor.loading()&&timer.elapsed()<30000){QApplication::processEvents();QThread::msleep(1);}ASSERT_FALSE(monitor.loading());QApplication::processEvents();EXPECT_TRUE(monitor.grab().save(output+"/Seguimiento-componentes.png"));
    const auto original=boundaryExample();auto rounded=original;ASSERT_TRUE(rounded.filletVertex(2,1));auto cfg=assignedExample();cfg.insert("boundaryGeometry",boundaryGeometry(original));QTimer::singleShot(0,[&](){auto* d=qobject_cast<QDialog*>(QApplication::activeModalWidget());ASSERT_NE(d,nullptr);QApplication::processEvents();EXPECT_TRUE(d->grab().save(output+"/Contornos-componentes.png"));auto* preview=dynamic_cast<BoundaryPreview*>(d->findChild<QWidget*>("flow-boundary-preview"));ASSERT_NE(preview,nullptr);const auto v=rounded.segmentPoint(2,.5);const auto anchor=preview->viewTransform().map(QPointF(v.xMm,v.yMm));for(int i=0;i<3;++i){QNativeGestureEvent pinch(Qt::ZoomNativeGesture,QPointingDevice::primaryPointingDevice(),2,anchor,anchor,anchor,1,{});QApplication::sendEvent(preview,&pinch);}QApplication::processEvents();EXPECT_TRUE(d->grab().save(output+"/Contornos-ampliados-componentes.png"));d->reject();});editFlowSetup(nullptr,rounded,cfg,2);
    SketchEditorDialog editor;editor.resize(1120,750);editor.setSketch(rounded);editor.show();QApplication::processEvents();EXPECT_TRUE(editor.grab().save(output+"/Croquis-componentes.png"));QApplication::removeTranslator(&translator);
}

TEST(MeshRegions, ControlsAndCutsSurviveGlobalChangesAndCancelledDialogs){
    auto shape=boundaryExample();QJsonObject mesh{{"sizeMm",10},{"wallSizeMm",2},{"wedgeAngle",5},{"engine","occ-regions-v1"},{"partitions",QJsonArray{QJsonObject{{"from",QJsonArray{20,0}},{"to",QJsonArray{20,5}}}}},{"regions",QJsonArray{QJsonObject{{"id","kept"},{"name","Tobera"},{"seed",QJsonArray{5,5}},{"sizeMm",3},{"method","quads"}}}}};MeshRegionsWidget widget(shape,mesh);widget.show();QApplication::processEvents();auto settings=widget.meshSettings();EXPECT_EQ(settings.value("partitions"),mesh.value("partitions"));EXPECT_EQ(settings.value("regions"),mesh.value("regions"));widget.setGlobalSettings(QJsonObject{{"sizeMm",20}});EXPECT_EQ(widget.meshSettings().value("sizeMm"),20);EXPECT_EQ(widget.meshSettings().value("regions"),mesh.value("regions"));
    auto* table=widget.findChild<QTableWidget*>("mesh-regions-table");ASSERT_NE(table,nullptr);ASSERT_EQ(table->rowCount(),1);static_cast<QDoubleSpinBox*>(table->cellWidget(0,1))->setValue(1.5);EXPECT_DOUBLE_EQ(widget.meshSettings().value("regions").toArray()[0].toObject().value("sizeMm").toDouble(),1.5);auto before=widget.meshSettings();
    const auto buttons=widget.findChildren<QToolButton*>();auto coordinates=std::find_if(buttons.begin(),buttons.end(),[](auto* b){return b->text()=="Cut coordinates…";});ASSERT_NE(coordinates,buttons.end());QTimer::singleShot(0,[](){auto* dialog=qobject_cast<QDialog*>(QApplication::activeModalWidget());ASSERT_NE(dialog,nullptr);dialog->reject();});(*coordinates)->click();EXPECT_EQ(widget.meshSettings(),before);EXPECT_FALSE(widget.busy());auto remove=std::find_if(buttons.begin(),buttons.end(),[](auto* b){return b->text()=="Remove cut";});ASSERT_NE(remove,buttons.end());(*remove)->click();EXPECT_TRUE(widget.meshSettings().value("partitions").toArray().isEmpty());EXPECT_TRUE(widget.meshSettings().value("regions").toArray().isEmpty());ASSERT_EQ(widget.meshSettings().value("unmatchedRegions").toArray().size(),1);EXPECT_DOUBLE_EQ(widget.meshSettings().value("unmatchedRegions").toArray()[0].toObject().value("sizeMm").toDouble(),1.5);
}
TEST(MeshRegions, NativePreviewRunsAsynchronouslyAndPersistsRegionsWhenExplicitlyRequested){
    const auto runtime=qEnvironmentVariable("VORTIA_GMSH_RUNTIME"),input=qEnvironmentVariable("VORTIA_MESH_PREVIEW_INPUT"),output=qEnvironmentVariable("VORTIA_UI_ARTIFACT_DIR");if(runtime.isEmpty()||input.isEmpty())GTEST_SKIP()<<"Optional native Gmsh preview QA";
    QTranslator translator;ASSERT_TRUE(translator.load(QStringLiteral(VORTIA_TEST_QM)));qApp->installTranslator(&translator);ThemeManager::apply(*qApp,ThemeManager::Theme::Dark);
    const auto cfg=QJsonDocument::fromJson(readFile(input+"/settings.json")).object();SketchEditorDialog loader;ASSERT_TRUE(loader.readSketchFile(input+"/sketch.json"));
    QTimer::singleShot(0,[&](){auto* dialog=qobject_cast<QDialog*>(QApplication::activeModalWidget());ASSERT_NE(dialog,nullptr);auto* widget=dialog->findChild<MeshRegionsWidget*>("flow-mesh-regions");ASSERT_NE(widget,nullptr);bool finished=false,ok=false;int beats=0;QTimer heartbeat;heartbeat.setInterval(5);QObject::connect(&heartbeat,&QTimer::timeout,[&](){++beats;});QObject::connect(widget,&MeshRegionsWidget::previewFinished,widget,[&](bool success){ok=success;finished=true;});heartbeat.start();widget->requestPreview();QElapsedTimer timer;timer.start();while(!finished&&timer.elapsed()<30000){QApplication::processEvents();QThread::msleep(1);}heartbeat.stop();EXPECT_TRUE(finished);EXPECT_TRUE(ok)<<widget->findChild<QLabel*>("mesh-preview-status")->text().toStdString();EXPECT_GT(beats,1);EXPECT_EQ(widget->meshSettings().value("regions").toArray().size(),3);EXPECT_FALSE(widget->busy());if(!output.isEmpty()){ASSERT_TRUE(QDir().mkpath(output));QApplication::processEvents();EXPECT_TRUE(dialog->grab().save(output+"/Malla-tres-regiones-componentes.png"));}
        auto* table=widget->findChild<QTableWidget*>("mesh-regions-table");ASSERT_NE(table,nullptr);const int initialCells=table->item(0,3)->text().toInt();static_cast<QDoubleSpinBox*>(table->cellWidget(0,1))->setValue(4);EXPECT_TRUE(widget->findChild<QLabel*>("mesh-preview-status")->text().contains(QString::fromUtf8("desactualizada")));finished=ok=false;widget->requestPreview();timer.restart();while(!finished&&timer.elapsed()<30000){QApplication::processEvents();QThread::msleep(1);}EXPECT_TRUE(ok);EXPECT_GT(table->item(0,3)->text().toInt(),initialCells);if(!output.isEmpty()){QApplication::processEvents();EXPECT_TRUE(dialog->grab().save(output+"/Malla-refinada-componentes.png"));}dialog->accept();});
    const auto saved=editFlowSetup(nullptr,loader.sketch(),cfg,0);EXPECT_EQ(saved.value("mesh").toObject().value("regions").toArray().size(),3);EXPECT_EQ(saved.value("mesh").toObject().value("partitions"),cfg.value("mesh").toObject().value("partitions"));EXPECT_EQ(saved.value("boundaries"),normalizedBoundaryNames(cfg.value("boundaries").toArray()));qApp->removeTranslator(&translator);
}

TEST(MeshRegions, AnOpenSketchCannotStartTheMesher){Sketch2D shape;ASSERT_TRUE(shape.addPoint({0,0}));ASSERT_TRUE(shape.addPoint({10,0}));MeshRegionsWidget widget(shape,QJsonObject{{"sizeMm",10},{"wallSizeMm",1},{"wedgeAngle",5}});widget.requestPreview();EXPECT_FALSE(widget.busy());EXPECT_TRUE(widget.findChild<QLabel*>("mesh-preview-status")->text().contains("Close the fluid contour"));}

TEST(MeshRegions, ProjectSaveAndReopenRetainIndependentZoneControls){
    QTemporaryDir folder;auto project=Project::create(folder.filePath("zones").toStdString(),"zones");ASSERT_TRUE(project);MainWindow first;project->setSetupJson(WorkspaceTestAccess::setupJson(first));ASSERT_TRUE(project->save());ASSERT_TRUE(WorkspaceTestAccess::open(first,QString::fromStdString(project->rootDir().string())));WorkspaceTestAccess::edit(first,boundaryExample(),{});auto cfg=assignedExample();QJsonObject mesh{{"sizeMm",10},{"wallSizeMm",2},{"wedgeAngle",5},{"engine","occ-regions-v1"},{"partitions",QJsonArray{QJsonObject{{"from",QJsonArray{20,0}},{"to",QJsonArray{20,5}}}}},{"regions",QJsonArray{QJsonObject{{"id","nozzle"},{"name","Tobera"},{"seed",QJsonArray{5,5}},{"sizeMm",2},{"method","quads"}},QJsonObject{{"id","external"},{"name","Exterior"},{"seed",QJsonArray{25,3}},{"sizeMm",15},{"method","triangles"}}}}};cfg.insert("mesh",mesh);WorkspaceTestAccess::flow(first,cfg);ASSERT_TRUE(WorkspaceTestAccess::save(first));MainWindow reopened;ASSERT_TRUE(WorkspaceTestAccess::open(reopened,QString::fromStdString(project->rootDir().string())));EXPECT_EQ(WorkspaceTestAccess::flow(reopened).value("mesh"),mesh);expectShape(WorkspaceTestAccess::editor(reopened).sketch(),boundaryExample());
}
