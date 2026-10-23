#include "vortia/ui/SketchEditorDialog.hpp"
#include "vortia/geometry/SketchTemplates.hpp"

#include <QBrush>
#include <QDialogButtonBox>
#include <QKeySequence>
#include <QShortcut>
#include <QToolButton>
#include <optional>
#include <utility>
#include <vector>
#include <QDoubleSpinBox>
#include <QFileDialog>
#include <QFile>
#include <QFileInfo>
#include <QStringList>
#include <QDir>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QInputDialog>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QMessageBox>
#include <QMouseEvent>
#include <QPainter>
#include <QPushButton>
#include <QSaveFile>
#include <QSplitter>
#include <QTableWidget>
#include <QVBoxLayout>

#include <algorithm>
#include <cmath>
#include <limits>

namespace vortia {

class SketchCanvas2D final : public QWidget {
public:
    explicit SketchCanvas2D(QWidget* parent = nullptr) : QWidget(parent) {
        // Keep the CAD canvas usable in a docked 1280x800 MacBook window.
        setMinimumSize(320, 260);
        setMouseTracking(true);
        setToolTip(tr("Click a vertex or segment to select it; measurements are in mm."));
    }
    void setSketch(const Sketch2D* sketch) { sketch_ = sketch; update(); }
    void setDrawMode(bool enabled) {
        drawMode_ = enabled;
        setCursor(enabled ? Qt::CrossCursor : Qt::ArrowCursor);
    }
    std::function<void(SketchPoint2D)> onCanvasClicked;
    void setSelected(int point, int segment) {
        selectedPoint_ = point;
        selectedSegment_ = segment;
        update();
    }
    std::function<void(int)> onPointSelected;
    std::function<void(int)> onSegmentSelected;
    std::function<void(int, SketchPoint2D)> onPointDragged;
protected:
    void paintEvent(QPaintEvent*) override {
        QPainter p(this);
        p.setRenderHint(QPainter::Antialiasing);
        p.fillRect(rect(), QColor(25, 31, 41));
        if (!sketch_) return;
        // Keep the camera transform fixed while a vertex is dragged.
        if (dragVertex_ < 0) computeBounds();
        p.setPen(QPen(QColor(51, 64, 82), 1));
        const double gridMm = 10.0;
        const double minX = std::floor(x0_ / gridMm) * gridMm;
        const double maxX = x0_ + width() / scale_;
        const double minY = std::floor(y0_ / gridMm) * gridMm;
        const double maxY = y0_ + height() / scale_;
        for (double x = minX; x <= maxX; x += gridMm) {
            const double px = project({x, 0.0}).x();
            p.drawLine(QPointF(px, 0), QPointF(px, height()));
        }
        for (double y = minY; y <= maxY; y += gridMm) {
            const double py = project({0.0, y}).y();
            p.drawLine(QPointF(0, py), QPointF(width(), py));
        }
        p.setPen(QPen(QColor(75, 117, 155), 1));
        p.drawLine(project({0, y0_}), project({0, maxY}));
        p.drawLine(project({x0_, 0}), project({maxX, 0}));

        for (std::size_t i = 0; i < sketch_->segmentCount(); ++i) {
            const auto& a = sketch_->points()[i];
            const auto& b = sketch_->points()[(i + 1) % sketch_->points().size()];
            const bool picked = static_cast<int>(i) == selectedSegment_;
            p.setPen(QPen(picked ? QColor(255, 184, 74) : QColor(87, 199, 239),
                          picked ? 3.2 : 2.0));
            p.drawLine(project(a), project(b));
            const QPointF m = (project(a) + project(b)) * 0.5;
            p.setPen(QColor(220, 230, 245));
            p.drawText(m + QPointF(5, -7),
                       QString::number(sketch_->segmentLength(i), 'f', 2) + " mm");
        }
        for (std::size_t i = 0; i < sketch_->points().size(); ++i) {
            const QPointF at = project(sketch_->points()[i]);
            p.setPen(Qt::NoPen);
            p.setBrush(static_cast<int>(i) == selectedPoint_
                           ? QColor(255, 184, 74) : QColor(242, 248, 254));
            p.drawEllipse(at, 4.5, 4.5);
            p.setPen(QColor(170, 195, 218));
            p.drawText(at + QPointF(6, 16), QString::number(i));
        }
    }
    void mousePressEvent(QMouseEvent* event) override {
        if (!sketch_ || event->button() != Qt::LeftButton) return;
        computeBounds();
        const QPointF pos = event->position();
        if (drawMode_) {
            // Pixels -> millimetres, snapped to a 1 mm engineering grid.
            SketchPoint2D world{x0_ + pos.x() / scale_,
                                y0_ + (height() - pos.y()) / scale_};
            world.xMm = std::round(world.xMm);
            world.yMm = std::round(world.yMm);
            if (onCanvasClicked) onCanvasClicked(world);
            event->accept();
            return;
        }
        int bestPoint = -1;
        double bestDistance = 12.0;
        for (std::size_t i = 0; i < sketch_->points().size(); ++i) {
            const double d = QLineF(pos, project(sketch_->points()[i])).length();
            if (d < bestDistance) { bestDistance = d; bestPoint = static_cast<int>(i); }
        }
        if (bestPoint >= 0) {
            dragVertex_ = bestPoint;
            if (onPointSelected) onPointSelected(bestPoint);
            event->accept();
            return;
        }
        int bestSegment = -1;
        double bestSegmentDistance = 13.0;
        for (std::size_t i = 0; i < sketch_->segmentCount(); ++i) {
            const QPointF a = project(sketch_->points()[i]);
            const QPointF b = project(sketch_->points()[(i + 1) % sketch_->points().size()]);
            const QPointF v = b - a;
            const QPointF w = pos - a;
            const double denom = QPointF::dotProduct(v, v);
            const double t = denom > 0 ? std::clamp(QPointF::dotProduct(w, v) / denom, 0.0, 1.0) : 0.0;
            const double d = QLineF(pos, a + t * v).length();
            if (d < bestSegmentDistance) {
                bestSegmentDistance = d;
                bestSegment = static_cast<int>(i);
            }
        }
        if (bestSegment >= 0 && onSegmentSelected) onSegmentSelected(bestSegment);
        event->accept();
    }
    void mouseMoveEvent(QMouseEvent* event) override {
        if (dragVertex_ >= 0 && (event->buttons() & Qt::LeftButton)) {
            const QPointF p = event->position();
            SketchPoint2D world{x0_ + p.x() / scale_,
                                y0_ + (height() - p.y()) / scale_};
            world.xMm = std::round(world.xMm);
            world.yMm = std::round(world.yMm);
            if (onPointDragged) onPointDragged(dragVertex_, world);
            event->accept();
            return;
        }
        if (!(event->buttons() & Qt::LeftButton) && dragVertex_ >= 0) {
            dragVertex_ = -1;
            update();
        }
        QWidget::mouseMoveEvent(event);
    }
    void leaveEvent(QEvent* event) override {
        dragVertex_ = -1;
        update();
        QWidget::leaveEvent(event);
    }
    void mouseReleaseEvent(QMouseEvent* event) override {
        if (event->button() == Qt::LeftButton && dragVertex_ >= 0) {
            dragVertex_ = -1;
            update();
            event->accept();
            return;
        }
        QWidget::mouseReleaseEvent(event);
    }
private:
    QPointF project(SketchPoint2D pt) const {
        return {(pt.xMm - x0_) * scale_, height() - (pt.yMm - y0_) * scale_};
    }
    void computeBounds() {
        // Include the origin and an initial 200 x 140 mm working area.
        double minX = 0.0, maxX = 200.0, minY = -70.0, maxY = 70.0;
        for (const auto& v : sketch_->points()) {
            minX = std::min(minX, v.xMm);
            maxX = std::max(maxX, v.xMm);
            minY = std::min(minY, v.yMm);
            maxY = std::max(maxY, v.yMm);
        }
        const double dx = std::max(1.0, maxX - minX);
        const double dy = std::max(1.0, maxY - minY);
        scale_ = std::max(0.1, std::min((width() - 65.0) / dx, (height() - 65.0) / dy));
        x0_ = (minX + maxX) * 0.5 - width() / (2.0 * scale_);
        y0_ = (minY + maxY) * 0.5 - height() / (2.0 * scale_);
    }
    const Sketch2D* sketch_{nullptr};
    bool drawMode_{false};
    int dragVertex_{-1};
    int selectedPoint_{-1};
    int selectedSegment_{-1};
    double scale_{1.0};
    double x0_{-100.0};
    double y0_{-70.0};
};

namespace {

std::optional<std::vector<double>> dimensionDialog(
    QWidget* parent, const QString& title,
    const std::vector<std::pair<QString, double>>& parameters) {
    QDialog dialog(parent);
    dialog.setWindowTitle(title);
    QVBoxLayout root(&dialog);
    QFormLayout form;
    std::vector<QDoubleSpinBox*> editors;
    for (const auto& [label, initial] : parameters) {
        auto* value = new QDoubleSpinBox(&dialog);
        value->setRange(0.001, 1000000.0);
        value->setDecimals(3);
        value->setValue(initial);
        value->setSuffix(QStringLiteral(" mm"));
        form.addRow(label, value);
        editors.push_back(value);
    }
    root.addLayout(&form);
    auto* buttons = new QDialogButtonBox(
        QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
    root.addWidget(buttons);
    QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
    QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    if (dialog.exec() != QDialog::Accepted) return std::nullopt;
    std::vector<double> values;
    for (auto* editor : editors) values.push_back(editor->value());
    return values;
}

} // namespace

SketchEditorDialog::SketchEditorDialog(QWidget* parent) : QDialog(parent) {
    setWindowTitle(tr("Vortia — Dimensioned 2D Sketch"));
    resize(1160, 710);
    auto* root = new QVBoxLayout(this);
    auto* intro = new QLabel(
        tr("Precise 2D sketch in millimetres. Select a segment to split it or enter an exact length. "
           "This sketch is not yet a CFD-ready mesh."), this);
    intro->setWordWrap(true);
    root->addWidget(intro);

    auto* horizontal = new QHBoxLayout;
    root->addLayout(horizontal, 1);
    canvas_ = new SketchCanvas2D(this);
    canvas_->setSketch(&sketch_);
    horizontal->addWidget(canvas_, 1);

    auto* sidebar = new QWidget(this);
    sidebar->setMinimumWidth(285);
    sidebar->setMaximumWidth(335);
    auto* tools = new QVBoxLayout(sidebar);
    auto* form = new QFormLayout;
    x_ = new QDoubleSpinBox(sidebar);
    y_ = new QDoubleSpinBox(sidebar);
    for (auto* control : {x_, y_}) {
        control->setRange(-1000000.0, 1000000.0);
        control->setDecimals(3);
        control->setSuffix(QStringLiteral(" mm"));
        control->setSingleStep(1.0);
    }
    form->addRow(tr("X coordinate"), x_);
    form->addRow(tr("Y coordinate"), y_);
    tools->addLayout(form);
    auto* draw = new QPushButton(tr("Draw with mouse"), sidebar);
    draw->setCheckable(true);
    draw->setToolTip(tr("Click the canvas to add points on a 1 mm grid. Turn off Draw to drag vertices."));
    auto* rectangle = new QPushButton(tr("Rectangle…"), sidebar);
    auto* circle = new QPushButton(tr("Circle…"), sidebar);
    auto* nozzle = new QPushButton(tr("Parametric nozzle…"), sidebar);
    auto* add = new QPushButton(tr("Add vertex"), sidebar);
    auto* move = new QPushButton(tr("Move selected vertex"), sidebar);
    auto* splitButton = new QPushButton(tr("Split selected segment"), sidebar);
    auto* dimension = new QPushButton(tr("Set exact segment length…"), sidebar);
    auto* close = new QPushButton(tr("Close / Open profile"), sidebar);
    for (auto* b : {draw, rectangle, circle, nozzle, add, move,
                    splitButton, dimension, close}) tools->addWidget(b);
    table_ = new QTableWidget(sidebar);
    table_->setColumnCount(3);
    table_->setHorizontalHeaderLabels({tr("Vertex"), tr("X (mm)"), tr("Y (mm)")});
    table_->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
    table_->setSelectionBehavior(QAbstractItemView::SelectRows);
    table_->setEditTriggers(QAbstractItemView::NoEditTriggers);
    tools->addWidget(table_, 1);
    selected_ = new QLabel(sidebar);
    selected_->setWordWrap(true);
    tools->addWidget(selected_);
    horizontal->addWidget(sidebar);

    auto* bottom = new QHBoxLayout;
    auto* loadButton = new QPushButton(tr("Load sketch…"), this);
    auto* saveButton = new QPushButton(tr("Save sketch…"), this);
    auto* clear = new QPushButton(tr("Clear sketch"), this);
    bottom->addWidget(loadButton);
    bottom->addWidget(saveButton);
    bottom->addWidget(clear);
    bottom->addStretch();
    auto* done = new QPushButton(tr("Close"), this);
    bottom->addWidget(done);
    root->addLayout(bottom);
    status_ = new QLabel(this);
    root->addWidget(status_);

    connect(draw, &QPushButton::toggled, this,
            [this](bool checked) { canvas_->setDrawMode(checked); });
    connect(rectangle, &QPushButton::clicked, this, [this]() {
        const auto dims = dimensionDialog(this, tr("Rectangle dimensions"),
                                         {{tr("Width"), 200.0}, {tr("Height"), 100.0}});
        if (!dims || !confirmReplacement()) return;
        const auto shape = makeRectangleSketch((*dims)[0], (*dims)[1]);
        if (shape) setShape(*shape);
    });
    connect(circle, &QPushButton::clicked, this, [this]() {
        const auto dims = dimensionDialog(this, tr("Circle dimensions"),
                                         {{tr("Diameter"), 100.0}});
        if (!dims || !confirmReplacement()) return;
        const auto shape = makeCircleSketch((*dims)[0]);
        if (shape) setShape(*shape);
    });
    connect(nozzle, &QPushButton::clicked, this, [this]() {
        const auto dims = dimensionDialog(this, tr("Nozzle dimensions"),
            {{tr("Inlet diameter"), nozzleDims_.inletDiameterMm},
             {tr("Throat diameter"), nozzleDims_.throatDiameterMm},
             {tr("Outlet diameter"), nozzleDims_.outletDiameterMm},
             {tr("Converging length"), nozzleDims_.convergingLengthMm},
             {tr("Diverging length"), nozzleDims_.divergingLengthMm}});
        if (!dims) return;
        const NozzleDimensions proposed{(*dims)[0],(*dims)[1],(*dims)[2],
                                        (*dims)[3],(*dims)[4]};
        const auto shape = makeNozzleProfile(proposed);
        if (!shape) {
            QMessageBox::warning(this, tr("Invalid nozzle"),
                tr("The throat diameter must be no greater than the inlet and outlet diameters."));
            return;
        }
        if (!confirmReplacement()) return;
        nozzleDims_ = proposed;
        setShape(*shape);
    });
    connect(add, &QPushButton::clicked, this, [this]() { addPoint(); });
    connect(move, &QPushButton::clicked, this, [this]() { movePoint(); });
    connect(splitButton, &QPushButton::clicked, this, [this]() { this->split(); });
    connect(dimension, &QPushButton::clicked, this, [this]() { changeLength(); });
    connect(close, &QPushButton::clicked, this, [this]() {
        if (sketch_.closed()) {
            sketch_.open();
            emit sketchChanged();
        } else if (!sketch_.close()) {
            QMessageBox::warning(this, tr("Invalid profile"),
                                 tr("At least three distinct vertices are needed."));
        } else {
            emit sketchChanged();
        }
        refresh();
    });
    connect(loadButton, &QPushButton::clicked, this, [this]() { load(); });
    connect(saveButton, &QPushButton::clicked, this, [this]() { save(); });
    connect(clear, &QPushButton::clicked, this, [this]() {
        sketch_.clear();
        selectedSegment_ = -1;
        selectedPoint_ = -1;
        emit sketchChanged();
        refresh();
    });
    connect(done, &QPushButton::clicked, this,
            [this]() { emit closeRequested(); });
    connect(table_, &QTableWidget::cellClicked, this, [this](int row, int) { setSelectedPoint(row); });
    canvas_->onPointSelected = [this](int index) { setSelectedPoint(index); };
    canvas_->onPointDragged = [this](int index, SketchPoint2D point) {
        if (sketch_.movePoint(static_cast<std::size_t>(index), point)) {
            selectedPoint_ = index;
            x_->setValue(point.xMm);
            y_->setValue(point.yMm);
            emit sketchChanged();
            refresh();
        }
    };
    canvas_->onCanvasClicked = [this](SketchPoint2D point) {
        if (sketch_.closed()) {
            QMessageBox::information(this, tr("Closed profile"),
                tr("Open the profile before adding points."));
            return;
        }
        if (sketch_.addPoint(point)) {
            x_->setValue(point.xMm);
            y_->setValue(point.yMm);
            emit sketchChanged();
            refresh();
        }
    };
    canvas_->onSegmentSelected = [this](int index) { setSelectedSegment(index); };
    refresh();
}

bool SketchEditorDialog::confirmReplacement() const {
    if (sketch_.points().empty()) return true;
    return QMessageBox::question(const_cast<SketchEditorDialog*>(this), tr("Replace sketch?"),
        tr("Replace the current 2D profile with the new parametric shape?"),
        QMessageBox::Yes | QMessageBox::No, QMessageBox::No) == QMessageBox::Yes;
}

void SketchEditorDialog::setShape(const Sketch2D& shape) {
    sketch_ = shape;
    selectedSegment_ = -1;
    selectedPoint_ = -1;
    emit sketchChanged();
    refresh();
}

void SketchEditorDialog::refresh() {
    table_->setRowCount(static_cast<int>(sketch_.points().size()));
    for (std::size_t i = 0; i < sketch_.points().size(); ++i) {
        const auto& p = sketch_.points()[i];
        table_->setItem(static_cast<int>(i), 0, new QTableWidgetItem(QString::number(i)));
        table_->setItem(static_cast<int>(i), 1, new QTableWidgetItem(QString::number(p.xMm, 'f', 3)));
        table_->setItem(static_cast<int>(i), 2, new QTableWidgetItem(QString::number(p.yMm, 'f', 3)));
    }
    selected_->setText(selectedSegment_ >= 0 && static_cast<std::size_t>(selectedSegment_) < sketch_.segmentCount()
        ? tr("Segment %1: %2 mm").arg(selectedSegment_)
                 .arg(sketch_.segmentLength(static_cast<std::size_t>(selectedSegment_)), 0, 'f', 3)
        : tr("Select a segment or vertex in the drawing."));
    status_->setText(tr("%1 vertices · %2 segments · Perimeter: %3 mm · %4")
        .arg(sketch_.points().size()).arg(sketch_.segmentCount())
        .arg(sketch_.perimeter(), 0, 'f', 3)
        .arg(sketch_.closed() ? tr("Closed profile") : tr("Open profile")));
    canvas_->setSelected(selectedPoint_, selectedSegment_);
}

void SketchEditorDialog::setSelectedSegment(int index) {
    selectedSegment_ = index;
    selectedPoint_ = -1;
    refresh();
}
void SketchEditorDialog::setSelectedPoint(int index) {
    if (index < 0 || static_cast<std::size_t>(index) >= sketch_.points().size()) return;
    selectedPoint_ = index;
    selectedSegment_ = -1;
    x_->setValue(sketch_.points()[static_cast<std::size_t>(index)].xMm);
    y_->setValue(sketch_.points()[static_cast<std::size_t>(index)].yMm);
    refresh();
}
void SketchEditorDialog::addPoint() {
    if (!sketch_.addPoint({x_->value(), y_->value()})) {
        QMessageBox::warning(this, tr("Invalid point"),
                             tr("Could not append the point. Open profiles and distinct consecutive points are required."));
        return;
    }
    emit sketchChanged();
    refresh();
}
void SketchEditorDialog::movePoint() {
    if (selectedPoint_ < 0 ||
        !sketch_.movePoint(static_cast<std::size_t>(selectedPoint_), {x_->value(), y_->value()})) {
        QMessageBox::warning(this, tr("Invalid point"), tr("Select a vertex and enter valid coordinates."));
        return;
    }
    emit sketchChanged();
    refresh();
}
void SketchEditorDialog::split() {
    if (selectedSegment_ < 0 ||
        !sketch_.splitSegment(static_cast<std::size_t>(selectedSegment_))) {
        QMessageBox::warning(this, tr("Select segment"), tr("Select a segment to split into two equal parts."));
        return;
    }
    ++selectedSegment_;
    emit sketchChanged();
    refresh();
}
void SketchEditorDialog::changeLength() {
    if (selectedSegment_ < 0 ||
        static_cast<std::size_t>(selectedSegment_) >= sketch_.segmentCount()) {
        QMessageBox::information(this, tr("Select segment"), tr("Select a segment first."));
        return;
    }
    const std::size_t index = static_cast<std::size_t>(selectedSegment_);
    bool ok = false;
    const double value = QInputDialog::getDouble(this, tr("Exact dimension"),
        tr("Length (mm)"), sketch_.segmentLength(index), 0.001, 1000000.0, 3, &ok);
    if (ok && !sketch_.setSegmentLength(index, value))
        QMessageBox::warning(this, tr("Dimension failed"),
                             tr("Could not set that length without creating a degenerate segment."));
    else if (ok)
        emit sketchChanged();
    refresh();
}
void SketchEditorDialog::setSketch(const Sketch2D& shape) {
    sketch_ = shape;
    selectedPoint_ = -1;
    selectedSegment_ = -1;
    refresh();
}

bool SketchEditorDialog::writeSketchFile(const QString& path) const {
    QJsonObject root{{QStringLiteral("schemaVersion"), 1},
                     {QStringLiteral("units"), QStringLiteral("mm")},
                     {QStringLiteral("closed"), sketch_.closed()}};
    QJsonArray vertices;
    for (const auto& p : sketch_.points())
        vertices.append(QJsonArray{p.xMm, p.yMm});
    root.insert(QStringLiteral("vertices"), vertices);
    root.insert(QStringLiteral("nozzleDimensions"), QJsonObject{
        {QStringLiteral("inletDiameterMm"), nozzleDims_.inletDiameterMm},
        {QStringLiteral("throatDiameterMm"), nozzleDims_.throatDiameterMm},
        {QStringLiteral("outletDiameterMm"), nozzleDims_.outletDiameterMm},
        {QStringLiteral("convergingLengthMm"), nozzleDims_.convergingLengthMm},
        {QStringLiteral("divergingLengthMm"), nozzleDims_.divergingLengthMm}
    });
    const QFileInfo info(path);
    if (!QDir().mkpath(info.absolutePath()))
        return false;
    QSaveFile file(path);
    return file.open(QIODevice::WriteOnly)
        && file.write(QJsonDocument(root).toJson(QJsonDocument::Indented)) >= 0
        && file.commit();
}

bool SketchEditorDialog::readSketchFile(const QString& path) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly))
        return false;
    QJsonParseError parse;
    const QJsonDocument document = QJsonDocument::fromJson(file.readAll(), &parse);
    const QJsonObject root = document.object();
    if (parse.error != QJsonParseError::NoError || !document.isObject() ||
        root.value(QStringLiteral("schemaVersion")).toInt() != 1 ||
        root.value(QStringLiteral("units")).toString() != QStringLiteral("mm"))
        return false;
    Sketch2D next;
    const QJsonArray vertices = root.value(QStringLiteral("vertices")).toArray();
    if (vertices.size() > 10000)
        return false;
    for (const auto& row : vertices) {
        const QJsonArray coords = row.toArray();
        if (coords.size() != 2 || !coords[0].isDouble() || !coords[1].isDouble()
            || !next.addPoint({coords[0].toDouble(), coords[1].toDouble()}))
            return false;
    }
    if (root.value(QStringLiteral("closed")).toBool() && !next.close())
        return false;
    if (root.contains(QStringLiteral("nozzleDimensions"))) {
        const auto value = root.value(QStringLiteral("nozzleDimensions"));
        if (!value.isObject()) return false;
        const auto n = value.toObject();
        const QStringList keys{
            QStringLiteral("inletDiameterMm"), QStringLiteral("throatDiameterMm"),
            QStringLiteral("outletDiameterMm"), QStringLiteral("convergingLengthMm"),
            QStringLiteral("divergingLengthMm")
        };
        for (const auto& key : keys)
            if (!n.value(key).isDouble()) return false;
        const NozzleDimensions loaded{
            n.value(keys[0]).toDouble(), n.value(keys[1]).toDouble(),
            n.value(keys[2]).toDouble(), n.value(keys[3]).toDouble(),
            n.value(keys[4]).toDouble()
        };
        if (!makeNozzleProfile(loaded)) return false;
        nozzleDims_ = loaded;
    }
    setSketch(next);
    return true;
}

void SketchEditorDialog::save() {
    const QString filename = QFileDialog::getSaveFileName(
        this, tr("Save 2D sketch"), {}, tr("Vortia sketch (*.vsketch.json)"));
    if (!filename.isEmpty() && !writeSketchFile(filename))
        QMessageBox::warning(this, tr("Save error"), tr("Could not save sketch."));
}

void SketchEditorDialog::load() {
    const QString filename = QFileDialog::getOpenFileName(
        this, tr("Load 2D sketch"), {}, tr("Vortia sketch (*.vsketch.json)"));
    if (filename.isEmpty())
        return;
    if (!readSketchFile(filename))
        QMessageBox::warning(this, tr("Load error"), tr("Invalid or unsupported sketch format."));
    else
        emit sketchChanged();
}

} // namespace vortia
