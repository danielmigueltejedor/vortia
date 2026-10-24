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
    WorkspaceTestAccess::add(window,"Empty project");
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
        d->findChild<QDoubleSpinBox*>("flow-solver-processes")->setValue(2);d->accept();
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
