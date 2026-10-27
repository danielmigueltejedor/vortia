#include "vortia/ui/UiIcon.hpp"
#include "vortia/ui/SketchEditorDialog.hpp"
#include "vortia/geometry/SketchTemplates.hpp"
#include "vortia/geometry/SketchSnapping.hpp"
#include <QApplication>
#include <array>
#include <QStyle>
#include <QClipboard>
#include <QMimeData>
#include <QAction>
#include <QMenu>
#include <QSettings>
#include <QStyledItemDelegate>
#include <QSignalBlocker>
#include <QTimer>
#include "vortia/mesh/PlanarNozzleBlockMesh.hpp"
#include "vortia/mesh/AxisymmetricNozzleBlockMesh.hpp"

#include <QBrush>
#include <QButtonGroup>
#include <QGridLayout>
#include <QNativeGestureEvent>
#include <QWheelEvent>
#include <QResizeEvent>
#include <QKeyEvent>
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
#include <QPainterPath>
#include <QPushButton>
#include <QSaveFile>
#include <QScrollArea>
#include <QSplitter>
#include <QTableWidget>
#include <QVBoxLayout>

#include <algorithm>
#include <cmath>
#include <limits>
#include <numbers>

namespace vortia {

class SketchCanvas2D final : public QWidget {
public:
    explicit SketchCanvas2D(QWidget* parent = nullptr) : QWidget(parent) {
        // Keep the CAD canvas usable in a docked 1280x800 MacBook window.
        setMinimumSize(320, 260);
        setMouseTracking(true);
        setObjectName(QStringLiteral("vortia-sketch-canvas"));
        setFocusPolicy(Qt::StrongFocus);
        setToolTip(SketchEditorDialog::tr("Pinch to zoom; swipe with two fingers to pan. Drag empty space, or hold Space to pan. Esc selects; L draws; F fits the sketch."));
    }
    void setSketch(const Sketch2D* sketch) { dragVertex_ = -1; sketch_ = sketch; fitView(); }
    void setInteractionPreferences(bool snap,double spacing,bool grid,bool dimensions,double zoom) {
        snap_=snap;snapSpacing_=std::clamp(spacing,.001,1000.0);showGrid_=grid;showDimensions_=dimensions;
        zoomSensitivity_=std::clamp(zoom,.25,3.0);update();
    }
    void fitView() { fitPending_ = true; update(); }
    void setDrawMode(bool enabled) {
        dragVertex_ = -1;
        drawMode_ = enabled;hoverVisible_=false;
        updateCursor();
    }
    void setPanMode(bool enabled) { panMode_ = enabled; dragVertex_ = -1; updateCursor(); }
    void setReadOnly(bool value) { readOnly_ = value; dragVertex_ = -1; if (value) setDrawMode(false); }
    SketchSnapOptions assistance;
    std::function<void()> onSelectRequested,onDeleteRequested,onEditStarted,onEditFinished,onUndoRequested,onRedoRequested,onCopyRequested,onPasteRequested;
    std::function<void(const QString&)> onHint;
    SketchSnapResult placement(QPointF pixel,Qt::KeyboardModifiers modifiers) const {
        SketchPoint2D world{x0_+pixel.x()/scale_,y0_+(height()-pixel.y())/scale_};
        auto options=assistance;if(modifiers&Qt::AltModifier)options.enabled=false;
        if(modifiers&Qt::ShiftModifier){options.enabled=true;options.orthogonal=true;}
        auto result=snapSketchPoint(*sketch_,world,10.0/scale_,options);
        if(result.kind==SketchSnapKind::None&&snap_){result.point.xMm=std::round(world.xMm/snapSpacing_)*snapSpacing_;result.point.yMm=std::round(world.yMm/snapSpacing_)*snapSpacing_;}
        return result;
    }
    QString hintName(SketchSnapKind kind) const {
        switch(kind){case SketchSnapKind::Endpoint:return SketchEditorDialog::tr("Endpoint");case SketchSnapKind::Midpoint:return SketchEditorDialog::tr("Midpoint");case SketchSnapKind::Center:return SketchEditorDialog::tr("Arc center");case SketchSnapKind::Alignment:return SketchEditorDialog::tr("Aligned with vertex");case SketchSnapKind::Perpendicular:return SketchEditorDialog::tr("Perpendicular");case SketchSnapKind::Orthogonal:return SketchEditorDialog::tr("Orthogonal");case SketchSnapKind::Polar:return SketchEditorDialog::tr("Polar tracking");default:return SketchEditorDialog::tr("Free / grid placement");}
    }
    std::function<void(SketchPoint2D)> onCanvasClicked;
    void setSelected(int point, int segment) {
        selectedPoint_ = point;
        selectedSegment_ = segment;
        update();
    }
    std::function<void(int)> onPointSelected,onPointEditRequested;
    std::function<void(int)> onSegmentSelected;
    std::function<void(int, SketchPoint2D)> onPointDragged;
protected:
    void paintEvent(QPaintEvent*) override {
        QPainter p(this);
        p.setRenderHint(QPainter::Antialiasing);
        const bool dark=palette().color(QPalette::Window).lightness()<128;
        const QColor background=dark?QColor("#141a22"):QColor("#edf0f4");
        const QColor grid=dark?QColor("#29323f"):QColor("#d8dfe6");
        const QColor ink=palette().color(QPalette::Text);
        const QColor accent=dark?QColor("#43abb8"):QColor("#177886");
        p.fillRect(rect(),background);
        if (!sketch_) return;
        ensureCamera();
        p.setPen(QPen(grid, 1));
        const double gridMm = std::pow(10.0, std::ceil(std::log10(20.0 / scale_)));
        const double minX = std::floor(x0_ / gridMm) * gridMm;
        const double maxX = x0_ + width() / scale_;
        const double minY = std::floor(y0_ / gridMm) * gridMm;
        const double maxY = y0_ + height() / scale_;
        if(showGrid_)for (double x = minX; x <= maxX; x += gridMm) {
            const double px = project({x, 0.0}).x();
            p.drawLine(QPointF(px, 0), QPointF(px, height()));
        }
        if(showGrid_)for (double y = minY; y <= maxY; y += gridMm) {
            const double py = project({0.0, y}).y();
            p.drawLine(QPointF(0, py), QPointF(width(), py));
        }
        p.setPen(QPen(palette().color(QPalette::PlaceholderText), 1));
        p.drawLine(project({0, y0_}), project({0, maxY}));
        p.drawLine(project({x0_, 0}), project({maxX, 0}));

        for (std::size_t i = 0; i < sketch_->segmentCount(); ++i) {
            const auto& a = sketch_->points()[i];
            const auto& b = sketch_->points()[(i + 1) % sketch_->points().size()];
            const bool picked = static_cast<int>(i) == selectedSegment_;
            p.setPen(QPen(picked ? accent : ink,
                          picked ? 2.6 : 1.6));
            if (const auto c = sketch_->arc(i)) {
                const QPointF center = project(c->center);
                const double radius = c->radiusMm * scale_;
                QPainterPath path;
                path.moveTo(project(a));
                path.arcTo(QRectF(center.x()-radius, center.y()-radius, 2*radius, 2*radius),
                           c->startRadians * 180.0 / std::numbers::pi,
                           c->sweepRadians * 180.0 / std::numbers::pi);
                p.drawPath(path);
            } else {
                p.drawLine(project(a), project(b));
            }
        }
        // Dense nozzle profiles have many short wall segments. Keep the
        // profile readable; selection always exposes its exact dimension.
        std::vector<QRectF> labels;
        const double outside = sketch_->closed() && sketch_->signedArea() < 0.0 ? -1.0 : 1.0;
        const auto labelSegment = [&](std::size_t i, bool selected) {
            if(!showDimensions_)return;
            const QPointF a = project(sketch_->points()[i]);
            const QPointF b = project(sketch_->points()[(i + 1) % sketch_->points().size()]);
            const auto arc = sketch_->arc(i);
            const QString text = arc
                ? QStringLiteral("R %1 mm").arg(arc->radiusMm, 0, 'f', 2)
                : QString::number(sketch_->segmentLength(i), 'f', 2) + QStringLiteral(" mm");
            const auto bounds = p.fontMetrics().boundingRect(text);
            if (!selected && QLineF(a, b).length() < bounds.width() + 16.0) return;
            const QPointF midpoint = project(sketch_->segmentPoint(i, 0.5));
            const QPointF delta = b - a;
            const double length = std::max(1.0, QLineF(a, b).length());
            const QPointF center = midpoint + outside * QPointF(-delta.y(), delta.x())
                / length * (bounds.height() * 0.5 + 12.0);
            QRectF label(0, 0, bounds.width() + 8.0, bounds.height() + 4.0);
            label.moveCenter(center);
            label.moveLeft(std::clamp(label.left(), 2.0, std::max(2.0, width() - label.width() - 2.0)));
            label.moveTop(std::clamp(label.top(), 2.0, std::max(2.0, height() - label.height() - 2.0)));
            if (!selected && std::any_of(labels.begin(), labels.end(),
                    [&](const QRectF& other) { return other.intersects(label.adjusted(-4, -4, 4, 4)); })) return;
            labels.push_back(label);
            p.fillRect(label, background);
            p.setPen(selected ? accent : ink);
            p.drawText(label, Qt::AlignCenter, text);
        };
        if (selectedSegment_ >= 0 && static_cast<std::size_t>(selectedSegment_) < sketch_->segmentCount())
            labelSegment(static_cast<std::size_t>(selectedSegment_), true);
        for (std::size_t i = 0; i < sketch_->segmentCount(); ++i)
            if (static_cast<int>(i) != selectedSegment_) labelSegment(i, false);
        for (std::size_t i = 0; i < sketch_->points().size(); ++i) {
            const QPointF at = project(sketch_->points()[i]);
            const bool picked = static_cast<int>(i) == selectedPoint_;
            p.setPen(Qt::NoPen);
            p.setBrush(picked
                           ? accent : ink);
            const double radius = picked || sketch_->points().size() <= 12 ? 4.5 : 2.5;
            p.drawEllipse(at, radius, radius);
            if (picked || sketch_->points().size() <= 12) {
                p.setPen(palette().color(QPalette::PlaceholderText));
                p.drawText(at + QPointF(6, 16), QString::number(i));
            }
        }
        if(drawMode_&&!readOnly_&&hoverVisible_){
            const auto at=project(hover_.point);p.setPen(QPen(accent,1,Qt::DashLine));
            for(const auto& ref:hover_.references){const auto q=project(ref);p.drawLine(q,at);p.drawEllipse(q,6,6);}
            if(!sketch_->points().empty())p.drawLine(project(sketch_->points().back()),at);
            p.setPen(QPen(accent,2));p.setBrush(Qt::NoBrush);p.drawRect(QRectF(at.x()-5,at.y()-5,10,10));
            QString label=hintName(hover_.kind);
            if(!sketch_->points().empty()){const auto a=sketch_->points().back();const double dx=hover_.point.xMm-a.xMm,dy=hover_.point.yMm-a.yMm;label+=QStringLiteral(" · %1 mm · %2°").arg(std::hypot(dx,dy),0,'f',3).arg(std::atan2(dy,dx)*180/std::numbers::pi,0,'f',1);}
            QRectF box=p.fontMetrics().boundingRect(label).adjusted(-7,-5,7,5);box.moveTopLeft(at+QPointF(14,-30));box.moveLeft(std::clamp(box.left(),0.0,std::max(0.0,width()-box.width())));box.moveTop(std::clamp(box.top(),0.0,std::max(0.0,height()-box.height())));
            p.fillRect(box,background);p.drawText(box,Qt::AlignCenter,label);
        }
    }
    void mousePressEvent(QMouseEvent* event) override {
        if (!sketch_) return;
        setFocus(Qt::MouseFocusReason);
        ensureCamera();
        const QPointF pos = event->position();
        if (event->button() == Qt::MiddleButton ||
            (event->button() == Qt::LeftButton && (panMode_ || spaceDown_))) {
            startPan(pos); event->accept(); return;
        }
        if (event->button() != Qt::LeftButton) return;
        if (drawMode_) {
            const auto snapped=placement(pos,event->modifiers());
            if (onCanvasClicked) onCanvasClicked(snapped.point);
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
            if (!readOnly_) {dragVertex_ = bestPoint;if(onEditStarted)onEditStarted();}
            if (onPointSelected) onPointSelected(bestPoint);
            event->accept();
            return;
        }
        int bestSegment = -1;
        double bestSegmentDistance = 13.0;
        for (std::size_t i = 0; i < sketch_->segmentCount(); ++i) {
            const int pieces = sketch_->arc(i) ? 48 : 1;
            for (int piece = 0; piece < pieces; ++piece) {
                const QPointF a = project(sketch_->segmentPoint(i, static_cast<double>(piece) / pieces));
                const QPointF b = project(sketch_->segmentPoint(i, static_cast<double>(piece+1) / pieces));
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
        }
        if (bestSegment >= 0) {
            if (onSegmentSelected) onSegmentSelected(bestSegment);
        } else {
            if (onPointSelected) onPointSelected(-1);
            startPan(pos);
        }
        event->accept();
    }
    void mouseDoubleClickEvent(QMouseEvent* event) override {
        if(!readOnly_&&!drawMode_&&sketch_){double best=12;int point=-1;for(std::size_t i=0;i<sketch_->points().size();++i){const double distance=QLineF(event->position(),project(sketch_->points()[i])).length();if(distance<best){best=distance;point=static_cast<int>(i);}}if(point>=0&&onPointEditRequested){if(onEditFinished)onEditFinished();dragVertex_=-1;onPointEditRequested(point);event->accept();return;}}
        QWidget::mouseDoubleClickEvent(event);
    }
    void mouseMoveEvent(QMouseEvent* event) override {
        if (panning_) {
            panBy(event->position() - panLast_);
            panLast_ = event->position();
            event->accept(); return;
        }
        if (dragVertex_ >= 0 && (event->buttons() & Qt::LeftButton)) {
            const QPointF p = event->position();
            SketchPoint2D world{x0_ + p.x() / scale_,
                                y0_ + (height() - p.y()) / scale_};
            if(snap_)world.xMm = std::round(world.xMm/snapSpacing_)*snapSpacing_;
            if(snap_)world.yMm = std::round(world.yMm/snapSpacing_)*snapSpacing_;
            if (onPointDragged) onPointDragged(dragVertex_, world);
            event->accept();
            return;
        }
        if (!(event->buttons() & Qt::LeftButton) && dragVertex_ >= 0) {
            dragVertex_ = -1;
            update();
        }
        if(drawMode_&&!readOnly_&&sketch_){hover_=placement(event->position(),event->modifiers());hoverVisible_=true;if(onHint)onHint(hintName(hover_.kind));update();}
        QWidget::mouseMoveEvent(event);
    }
    void leaveEvent(QEvent* event) override {
        hoverVisible_=false;update();QWidget::leaveEvent(event);
    }
    void mouseReleaseEvent(QMouseEvent* event) override {
        if (panning_ && (event->button() == Qt::LeftButton || event->button() == Qt::MiddleButton)) {
            panning_ = false; updateCursor(); event->accept(); return;
        }
        if (event->button() == Qt::LeftButton && dragVertex_ >= 0) {
            if(onEditFinished)onEditFinished();dragVertex_ = -1;
            update();
            event->accept();
            return;
        }
        QWidget::mouseReleaseEvent(event);
    }
    void wheelEvent(QWheelEvent* event) override {
        ensureCamera();
        if (event->modifiers() & Qt::ControlModifier) {
            const double amount = event->pixelDelta().isNull()
                ? event->angleDelta().y() / 120.0 : event->pixelDelta().y() / 40.0;
            zoomAt(event->position(), std::exp(amount * 0.18 * zoomSensitivity_));
        } else if (!event->pixelDelta().isNull()) {
            panBy(event->pixelDelta());
        } else if (event->source() != Qt::MouseEventNotSynthesized || event->phase() != Qt::NoScrollPhase) {
            panBy(QPointF(event->angleDelta()) / 3.0);
        } else {
            zoomAt(event->position(), std::exp(event->angleDelta().y() / 120.0 * 0.18 * zoomSensitivity_));
        }
        event->accept();
    }
    bool event(QEvent* event) override {
        if (event->type() == QEvent::NativeGesture) {
            auto* gesture = static_cast<QNativeGestureEvent*>(event);
            ensureCamera();
            if (gesture->gestureType() == Qt::ZoomNativeGesture)
                zoomAt(gesture->position(), std::pow(1.0+gesture->value(),zoomSensitivity_));
            else if (gesture->gestureType() == Qt::PanNativeGesture)
                panBy(gesture->delta());
            // Consume the entire native gesture sequence to avoid scrolling parents.
            event->accept(); return true;
        }
        if (event->type() == QEvent::FocusOut) {
            if(dragVertex_>=0&&onEditFinished)onEditFinished();spaceDown_ = false; panning_ = false; dragVertex_ = -1; updateCursor();
        }
        return QWidget::event(event);
    }
    void keyPressEvent(QKeyEvent* event) override {
        if(event->matches(QKeySequence::Undo)){if(onUndoRequested)onUndoRequested();event->accept();return;}
        if(event->matches(QKeySequence::Redo)){if(onRedoRequested)onRedoRequested();event->accept();return;}
        if(event->matches(QKeySequence::Copy)){if(onCopyRequested)onCopyRequested();event->accept();return;}
        if(event->matches(QKeySequence::Paste)){if(onPasteRequested)onPasteRequested();event->accept();return;}
        if((event->key()==Qt::Key_Return||event->key()==Qt::Key_Enter)&&selectedPoint_>=0&&!readOnly_){if(onPointEditRequested)onPointEditRequested(selectedPoint_);event->accept();return;}
        if(event->key()==Qt::Key_Delete||event->key()==Qt::Key_Backspace){if(!readOnly_&&onDeleteRequested)onDeleteRequested();event->accept();return;}
        if (event->key() == Qt::Key_Space) {
            spaceDown_ = true; updateCursor(); event->accept(); return;
        }
        if (event->key() == Qt::Key_Escape) {
            if (onSelectRequested) onSelectRequested();
            event->accept(); return;
        }
        QWidget::keyPressEvent(event);
    }
    void keyReleaseEvent(QKeyEvent* event) override {
        if (event->key() == Qt::Key_Space && !event->isAutoRepeat()) {
            spaceDown_ = false; updateCursor(); event->accept(); return;
        }
        QWidget::keyReleaseEvent(event);
    }
    void resizeEvent(QResizeEvent* event) override {
        if (!fitPending_ && event->oldSize().isValid()) {
            x0_ -= (event->size().width() - event->oldSize().width()) / (2.0 * scale_);
            y0_ -= (event->size().height() - event->oldSize().height()) / (2.0 * scale_);
        }
        QWidget::resizeEvent(event);
    }
private:
    void ensureCamera() { if (fitPending_ && sketch_) { computeBounds(); fitPending_ = false; } }
    void updateCursor() {
        setCursor(panning_ ? Qt::ClosedHandCursor : (panMode_ || spaceDown_) ? Qt::OpenHandCursor
                           : drawMode_ ? Qt::CrossCursor : Qt::ArrowCursor);
    }
    void startPan(QPointF pos) { panning_ = true; panLast_ = pos; dragVertex_ = -1; updateCursor(); }
    void panBy(QPointF delta) {
        x0_ -= delta.x() / scale_; y0_ += delta.y() / scale_; update();
    }
    void zoomAt(QPointF pos, double factor) {
        if (!std::isfinite(factor) || factor <= 0.0) return;
        const double next = std::clamp(scale_ * factor, 1e-4, 1e5);
        x0_ += pos.x() / scale_ - pos.x() / next;
        y0_ += (height() - pos.y()) / scale_ - (height() - pos.y()) / next;
        scale_ = next; update();
    }
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
        for (std::size_t i = 0; i < sketch_->segmentCount(); ++i) {
            if (const auto c = sketch_->arc(i)) {
                // Include all axis extrema that lie on the signed arc sweep.
                for (int quadrant = 0; quadrant < 4; ++quadrant) {
                    double t = quadrant * std::numbers::pi / 2.0 - c->startRadians;
                    t = c->sweepRadians > 0 ? std::fmod(t + 4*std::numbers::pi, 2*std::numbers::pi)
                                           : -std::fmod(-t + 4*std::numbers::pi, 2*std::numbers::pi);
                    if (std::abs(t) > std::abs(c->sweepRadians)) continue;
                    const auto v = sketch_->segmentPoint(i, t / c->sweepRadians);
                    minX = std::min(minX, v.xMm); maxX = std::max(maxX, v.xMm);
                    minY = std::min(minY, v.yMm); maxY = std::max(maxY, v.yMm);
                }
            }
        }
        const double dx = std::max(1.0, maxX - minX);
        const double dy = std::max(1.0, maxY - minY);
        scale_ = std::max(1e-9, std::min((width() - 65.0) / dx, (height() - 65.0) / dy));
        x0_ = (minX + maxX) * 0.5 - width() / (2.0 * scale_);
        y0_ = (minY + maxY) * 0.5 - height() / (2.0 * scale_);
    }
    const Sketch2D* sketch_{nullptr};
    bool drawMode_{false};
    bool readOnly_{false};
    bool snap_=true,showGrid_=true,showDimensions_=true;
    double snapSpacing_=1,zoomSensitivity_=1;
    bool panMode_{false};
    bool panning_{false};
    bool spaceDown_{false};
    bool fitPending_{true};
    QPointF panLast_;
    SketchSnapResult hover_;bool hoverVisible_=false;
    int dragVertex_{-1};
    int selectedPoint_{-1};
    int selectedSegment_{-1};
    double scale_{1.0};
    double x0_{-100.0};
    double y0_{-70.0};
};

namespace {

class CoordinateDelegate final:public QStyledItemDelegate {
public:using QStyledItemDelegate::QStyledItemDelegate;
    QWidget* createEditor(QWidget* parent,const QStyleOptionViewItem&,const QModelIndex&)const override{auto* spin=new QDoubleSpinBox(parent);spin->setRange(-2000000,2000000);spin->setDecimals(6);spin->setFrame(false);return spin;}
    void setEditorData(QWidget* editor,const QModelIndex& index)const override{static_cast<QDoubleSpinBox*>(editor)->setValue(index.data(Qt::EditRole).toDouble());}
    void setModelData(QWidget* editor,QAbstractItemModel* model,const QModelIndex& index)const override{auto* spin=static_cast<QDoubleSpinBox*>(editor);spin->interpretText();model->setData(index,spin->value(),Qt::EditRole);}
};

// Small CAD glyphs are drawn as vectors at both standard and Retina sizes.
enum class SketchGlyph { Select, Line, Pan, Fit, Rectangle, Circle, Nozzle,
                         Export, Add, Move, Split, Dimension, Close, Fillet, Radius };
QIcon sketchIcon(SketchGlyph glyph) {
    static const std::array<const char*,15> names{"select","line","pan","fit","rectangle","circle","nozzle",
        "export-image","vertex-add","vertex-move","segment-split","dimension","profile-close","fillet","radius"};
    return uiIcon(QString(":/vortia/icons/%1.svg").arg(names.at(static_cast<std::size_t>(glyph))));
}

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
    committed_={sketch_,nozzleDims_};
    connect(this,&SketchEditorDialog::sketchChanged,this,&SketchEditorDialog::recordEdit);
    setWindowTitle(tr("Vortia — Dimensioned 2D Sketch"));
    resize(1160, 710);
    auto* root = new QVBoxLayout(this);
    auto* intro = new QLabel(
        tr("Precise 2D sketch in millimetres. Select a segment to split it or enter an exact length. "
           "This sketch is not yet a CFD-ready mesh."), this);
    intro->setWordWrap(true);
    intro->setObjectName(QStringLiteral("vortia-sketch-intro"));
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
    x_->setObjectName("vortia-sketch-x");y_->setObjectName("vortia-sketch-y");
    for (auto* control : {x_, y_}) {
        control->setRange(-2000000.0, 2000000.0);
        control->setDecimals(6);
        control->setSuffix(QStringLiteral(" mm"));
        control->setSingleStep(1.0);
    }
    form->addRow(tr("X coordinate"), x_);
    form->addRow(tr("Y coordinate"), y_);
    tools->addLayout(form);
    auto* modeRow = new QHBoxLayout;
    auto* grid = new QGridLayout;
    const auto tool = [&](const QString& name, const char* id, SketchGlyph glyph) {
        auto* button = new QPushButton(sidebar);
        button->setObjectName(QString::fromLatin1(id));
        button->setAccessibleName(name); button->setToolTip(name);
        button->setIcon(sketchIcon(glyph)); button->setIconSize(QSize(22,22));
        button->setFixedSize(48,44); button->setAutoDefault(false);
        return button;
    };
    auto* select = tool(tr("Select (Esc / V)"), "vortia-sketch-select", SketchGlyph::Select);
    auto* draw = tool(tr("Draw polyline (L)"), "vortia-sketch-draw", SketchGlyph::Line);
    auto* pan = tool(tr("Pan (H / hold Space)"), "vortia-sketch-pan", SketchGlyph::Pan);
    auto* fit = tool(tr("Fit sketch (F)"), "vortia-sketch-fit", SketchGlyph::Fit);
    auto* modes = new QButtonGroup(this); modes->setExclusive(true);
    for (auto* b : {select, draw, pan}) {
        b->setCheckable(true);

        modes->addButton(b); modeRow->addWidget(b);
    }
    select->setChecked(true); modeRow->addWidget(fit); modeRow->addStretch();
    tools->insertLayout(0, modeRow);
    auto* modeHint = new QLabel(tr("Select: drag vertices or empty space."), sidebar);
    modeHint->setObjectName(QStringLiteral("vortia-sketch-mode-hint"));
    modeHint->setWordWrap(true); tools->insertWidget(1, modeHint);
    auto* rectangle = tool(tr("Rectangle…"), "vortia-sketch-rectangle", SketchGlyph::Rectangle);
    auto* circle = tool(tr("Circle…"), "vortia-sketch-circle", SketchGlyph::Circle);
    auto* nozzle = tool(tr("Parametric nozzle…"), "vortia-sketch-nozzle", SketchGlyph::Nozzle);
    auto* exportMeshButton = tool(tr("Export planar OpenFOAM mesh…"), "vortia-sketch-export", SketchGlyph::Export);
    exportMeshButton->setToolTip(tr("Export a 2D planar blockMeshDict (not an axisymmetric wedge or CFD case)."));
    auto* exportWedgeButton = tool(tr("Export axisymmetric wedge mesh…"), "vortia-sketch-export-wedge", SketchGlyph::Export);
    exportWedgeButton->setToolTip(tr("Export a collapsed-axis 5-degree wedge blockMeshDict (mesh only)."));
    auto* add = tool(tr("Add vertex"), "vortia-sketch-add", SketchGlyph::Add);
    auto* move = tool(tr("Move selected vertex"), "vortia-sketch-move", SketchGlyph::Move);
    auto* splitButton = tool(tr("Split selected segment"), "vortia-sketch-split", SketchGlyph::Split);
    auto* dimension = tool(tr("Set exact segment length…"), "vortia-sketch-dimension", SketchGlyph::Dimension);
    auto* close = tool(tr("Close / Open profile"), "vortia-sketch-close-profile", SketchGlyph::Close);
    auto* round = tool(tr("Round vertex…"), "vortia-sketch-fillet", SketchGlyph::Fillet);
    auto* radius = tool(tr("Arc radius…"), "vortia-sketch-radius", SketchGlyph::Radius);
    int cell = 0;
    for (auto* b : {rectangle, circle, nozzle, add, move, splitButton, dimension, round, radius, close, exportMeshButton, exportWedgeButton}) {
        grid->addWidget(b, cell / 4, cell % 4); ++cell;
    }
    tools->addLayout(grid);
    auto* editRow=new QHBoxLayout;
    const auto editCommand=[&](const char* text,const char* id,const QKeySequence& key,QStyle::StandardPixmap icon,auto callback){
        auto* action=new QAction(tr(text),this);action->setObjectName(QString("vortia-sketch-%1-action").arg(id));
        if(!key.isEmpty()){action->setShortcut(key);action->setShortcutContext(Qt::WidgetWithChildrenShortcut);addAction(action);}
        connect(action,&QAction::triggered,this,callback);
        auto* button=new QToolButton(sidebar);button->setObjectName(QString("vortia-sketch-%1").arg(id));const auto glyph = uiIcon(QString(":/vortia/icons/%1.svg").arg(id));action->setIcon(glyph.isNull()?style()->standardIcon(icon):glyph);button->setDefaultAction(action);button->setIconSize({22,22});button->setMinimumSize(38,36);button->setToolTip(action->text()+" · "+key.toString(QKeySequence::NativeText));editRow->addWidget(button);return action;
    };
    undoAction_=editCommand("Undo","undo",QKeySequence::Undo,QStyle::SP_ArrowBack,[this](){undoEdit();});
    redoAction_=editCommand("Redo","redo",QKeySequence::Redo,QStyle::SP_ArrowForward,[this](){redoEdit();});
    copyAction_=editCommand("Copy geometry","copy",QKeySequence::Copy,QStyle::SP_FileIcon,[this](){copySelection();});
    pasteAction_=editCommand("Paste geometry…","paste",QKeySequence::Paste,QStyle::SP_DialogOpenButton,[this](){pasteGeometry();});
    deleteAction_=editCommand("Delete selected vertex","delete",{},QStyle::SP_TrashIcon,[this](){deleteSelectedPoint();});
    tools->addLayout(editRow);
    auto* references=new QToolButton(sidebar);references->setObjectName("vortia-sketch-references");references->setText(tr("Drawing references"));references->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);references->setIcon(uiIcon(":/vortia/icons/probe.svg"));references->setPopupMode(QToolButton::InstantPopup);
    auto* referenceMenu=new QMenu(references);references->setMenu(referenceMenu);QSettings preferences("Vortia","Vortia");
    const auto option=[&](const char* text,const char* key,bool SketchSnapOptions::*member,bool initial,const QKeySequence& shortcut=QKeySequence{}){
        auto* action=referenceMenu->addAction(tr(text));action->setObjectName(QString("vortia-snap-%1").arg(key));action->setCheckable(true);action->setChecked(preferences.value(QString("sketch/assistance/%1").arg(key),initial).toBool());canvas_->assistance.*member=action->isChecked();
        if(!shortcut.isEmpty()){action->setShortcut(shortcut);action->setShortcutContext(Qt::WidgetWithChildrenShortcut);addAction(action);}
        connect(action,&QAction::toggled,this,[this,member,key](bool value){canvas_->assistance.*member=value;QSettings p("Vortia","Vortia");p.setValue(QString("sketch/assistance/%1").arg(key),value);canvas_->update();});return action;
    };
    option("Enable references (F3)","enabled",&SketchSnapOptions::enabled,true,QKeySequence(Qt::Key_F3));referenceMenu->addSeparator();
    option("Endpoints","endpoints",&SketchSnapOptions::endpoints,true);option("Midpoints","midpoints",&SketchSnapOptions::midpoints,true);option("Arc centers","centers",&SketchSnapOptions::centers,true);option("Vertex alignment","alignment",&SketchSnapOptions::alignment,true);option("Perpendicular tracking","perpendicular",&SketchSnapOptions::perpendicular,true);
    referenceMenu->addSeparator();option("Orthogonal mode (F8)","orthogonal",&SketchSnapOptions::orthogonal,false,QKeySequence(Qt::Key_F8));option("Polar tracking · 15° (F10)","polar",&SketchSnapOptions::polar,false,QKeySequence(Qt::Key_F10));tools->addWidget(references);
    auto* hint=new QLabel(tr("Shift: temporary orthogonal · Alt: suspend references"),sidebar);hint->setObjectName("vortia-sketch-snap-hint");hint->setWordWrap(true);tools->addWidget(hint);canvas_->onHint=[hint](const QString& text){hint->setText(text);};

    table_ = new QTableWidget(sidebar);
    table_->setColumnCount(3);
    table_->setHorizontalHeaderLabels({tr("Vertex"), tr("X (mm)"), tr("Y (mm)")});
    table_->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
    table_->setSelectionBehavior(QAbstractItemView::SelectRows);
    table_->setEditTriggers(QAbstractItemView::DoubleClicked|QAbstractItemView::EditKeyPressed);
    table_->setItemDelegateForColumn(1,new CoordinateDelegate(table_));table_->setItemDelegateForColumn(2,new CoordinateDelegate(table_));
    tools->addWidget(table_, 1);table_->installEventFilter(this);
    selected_ = new QLabel(sidebar);
    selected_->setWordWrap(true);
    tools->addWidget(selected_);
    // Keep the embedded sketch from forcing the entire main window taller
    // than a laptop screen, even while the CFD page is active.
    auto* toolScroll = new QScrollArea(this);
    toolScroll->setObjectName(QStringLiteral("vortia-sketch-tools-scroll"));
    toolScroll->setWidgetResizable(true);
    toolScroll->setFrameShape(QFrame::NoFrame);
    toolScroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    toolScroll->setObjectName("vortia-sketch-inspector");
    toolScroll->setMinimumWidth(305);
    toolScroll->setMaximumWidth(355);
    toolScroll->setWidget(sidebar);
    horizontal->addWidget(toolScroll);

    auto* bottom = new QHBoxLayout;
    auto* loadButton = new QPushButton(tr("Load sketch…"), this);
    loadButton->setObjectName(QStringLiteral("vortia-sketch-load"));
    auto* saveButton = new QPushButton(tr("Save sketch…"), this);saveButton->setObjectName("vortia-sketch-save");
    auto* clear = new QPushButton(tr("Clear sketch"), this);
    clear->setObjectName(QStringLiteral("vortia-sketch-clear"));
    bottom->addWidget(loadButton);
    bottom->addWidget(saveButton);
    bottom->addWidget(clear);
    bottom->addStretch();
    auto* done = new QPushButton(tr("Close"), this);done->setObjectName("vortia-sketch-dismiss");
    bottom->addWidget(done);
    root->addLayout(bottom);
    status_ = new QLabel(this);
    root->addWidget(status_);

    connect(select, &QPushButton::toggled, this, [this, modeHint](bool checked) {
        if (!checked) return;
        canvas_->setDrawMode(false); canvas_->setPanMode(false);
        modeHint->setText(readOnly_ ? tr("Case geometry: select to inspect; drag empty space to pan.")
                                    : tr("Select: drag vertices or empty space."));
    });
    connect(draw, &QPushButton::toggled, this, [this, modeHint](bool checked) {
        if (!checked) return;
        canvas_->setPanMode(false); canvas_->setDrawMode(true);
        modeHint->setText(tr("Draw: click to add vertices. Esc returns to selection."));
    });
    connect(pan, &QPushButton::toggled, this, [this, modeHint](bool checked) {
        if (!checked) return;
        canvas_->setDrawMode(false); canvas_->setPanMode(true);
        modeHint->setText(tr("Pan: drag the view. Pinch to zoom."));
    });
    connect(fit, &QPushButton::clicked, this, [this] { canvas_->fitView(); });
    canvas_->onSelectRequested = [select] { select->click(); };
    const auto shortcut = [this](const QKeySequence& key, QPushButton* button) {
        auto* action = new QShortcut(key, this);
        action->setContext(Qt::WidgetWithChildrenShortcut);
        connect(action, &QShortcut::activated, button, &QPushButton::click);
    };
    shortcut(QKeySequence(Qt::Key_Escape), select);
    shortcut(QKeySequence(Qt::Key_V), select); shortcut(QKeySequence(Qt::Key_L), draw);
    shortcut(QKeySequence(Qt::Key_H), pan); shortcut(QKeySequence(Qt::Key_F), fit);
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
    connect(exportMeshButton, &QPushButton::clicked, this, &SketchEditorDialog::exportNozzleMesh);
    connect(exportWedgeButton, &QPushButton::clicked, this, &SketchEditorDialog::exportWedgeMesh);
    connect(add, &QPushButton::clicked, this, [this]() { addPoint(); });
    connect(move, &QPushButton::clicked, this, [this]() { movePoint(); });
    connect(splitButton, &QPushButton::clicked, this, [this]() { this->split(); });
    connect(dimension, &QPushButton::clicked, this, [this]() { changeLength(); });
    connect(round, &QPushButton::clicked, this, &SketchEditorDialog::fillet);
    connect(radius, &QPushButton::clicked, this, &SketchEditorDialog::changeRadius);
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
    canvas_->onUndoRequested=[this](){undoEdit();};canvas_->onRedoRequested=[this](){redoEdit();};canvas_->onCopyRequested=[this](){copySelection();};canvas_->onPasteRequested=[this](){pasteGeometry();};
    canvas_->onDeleteRequested=[this](){deleteSelectedPoint();};
    canvas_->onEditStarted=[this](){dragEditing_=true;dragRecorded_=false;};
    canvas_->onEditFinished=[this](){dragEditing_=false;dragRecorded_=false;};
    connect(QApplication::clipboard(),&QClipboard::dataChanged,this,&SketchEditorDialog::refreshEditActions);
    canvas_->onPointEditRequested=[this](int index){editVertexCoordinates(index);};
    const auto applyCoordinates=[this](){if(readOnly_||selectedPoint_<0||sketch_.pointLockedByArcs(static_cast<std::size_t>(selectedPoint_)))return;const auto current=sketch_.points()[static_cast<std::size_t>(selectedPoint_)];if(std::abs(x_->value()-current.xMm)>5e-7||std::abs(y_->value()-current.yMm)>5e-7)movePoint();};
    connect(x_,&QDoubleSpinBox::editingFinished,this,applyCoordinates);connect(y_,&QDoubleSpinBox::editingFinished,this,applyCoordinates);
    connect(table_,&QTableWidget::itemChanged,this,[this](QTableWidgetItem* item){if(readOnly_||!item||item->column()==0)return;const auto index=static_cast<std::size_t>(item->row());if(index>=sketch_.points().size())return;bool validX=false,validY=false;const double x=table_->item(item->row(),1)->data(Qt::EditRole).toDouble(&validX),y=table_->item(item->row(),2)->data(Qt::EditRole).toDouble(&validY);if(validX&&validY&&sketch_.movePoint(index,{x,y})){selectedPoint_=item->row();selectedSegment_=-1;x_->setValue(x);y_->setValue(y);emit sketchChanged();}else status_->setText(tr("Those coordinates would create an invalid segment. The original vertex was retained."));QTimer::singleShot(0,this,[this](){refresh();});});
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
            status_->setText(tr("Open the profile before adding points."));findChild<QPushButton*>("vortia-sketch-select")->click();
            return;
        }
        if(sketch_.points().size()>=3&&std::hypot(point.xMm-sketch_.points().front().xMm,point.yMm-sketch_.points().front().yMm)<1e-6){if(sketch_.close()){findChild<QPushButton*>("vortia-sketch-select")->click();emit sketchChanged();refresh();}return;}
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
    if (auto* select = findChild<QPushButton*>(QStringLiteral("vortia-sketch-select"))) select->click();
    selectedSegment_ = -1;
    selectedPoint_ = -1;
    canvas_->fitView();
    emit sketchChanged();
    refresh();
}

void SketchEditorDialog::refresh() {
    QSignalBlocker tableSignals(table_);
    table_->setRowCount(static_cast<int>(sketch_.points().size()));
    for (std::size_t i = 0; i < sketch_.points().size(); ++i) {
        const auto& p = sketch_.points()[i];
        auto* id=new QTableWidgetItem(QString::number(i));id->setFlags(id->flags()&~Qt::ItemIsEditable);table_->setItem(static_cast<int>(i),0,id);
        for(int column=1;column<=2;++column){auto* cell=new QTableWidgetItem;cell->setData(Qt::EditRole,column==1?p.xMm:p.yMm);if(readOnly_||sketch_.pointLockedByArcs(i)){cell->setFlags(cell->flags()&~Qt::ItemIsEditable);cell->setToolTip(tr("This vertex supports a tangent arc. Select the arc and edit its radius."));}else cell->setToolTip(tr("Double click to edit this coordinate in millimetres."));table_->setItem(static_cast<int>(i),column,cell);}
    }
    selected_->setText(selectedSegment_ >= 0 && static_cast<std::size_t>(selectedSegment_) < sketch_.segmentCount()
        ? tr("Segment %1: %2 mm").arg(selectedSegment_+1)
                 .arg(sketch_.segmentLength(static_cast<std::size_t>(selectedSegment_)), 0, 'f', 3)
        : tr("Select a segment or vertex in the drawing."));
    if (selectedSegment_ >= 0) {
        if (const auto c = sketch_.arc(static_cast<std::size_t>(selectedSegment_)))
            selected_->setText(tr("Arc %1: radius %2 mm · length %3 mm")
                .arg(selectedSegment_+1).arg(c->radiusMm, 0, 'f', 3)
                .arg(sketch_.segmentLength(static_cast<std::size_t>(selectedSegment_)), 0, 'f', 3));
    } else if (selectedPoint_ >= 0 && sketch_.pointLockedByArcs(static_cast<std::size_t>(selectedPoint_))) {
        selected_->setText(tr("This vertex supports a tangent arc. Select the arc and edit its radius."));
    }
    status_->setText(tr("%1 vertices · %2 segments · Perimeter: %3 mm · %4")
        .arg(sketch_.points().size()).arg(sketch_.segmentCount())
        .arg(sketch_.perimeter(), 0, 'f', 3)
        .arg(sketch_.closed() ? tr("Closed profile") : tr("Open profile")));
    const bool locked=selectedPoint_>=0&&sketch_.pointLockedByArcs(static_cast<std::size_t>(selectedPoint_));x_->setReadOnly(readOnly_||locked);y_->setReadOnly(readOnly_||locked);
    if(auto* move=findChild<QPushButton*>("vortia-sketch-move"))move->setEnabled(!readOnly_&&selectedPoint_>=0&&!locked);
    canvas_->setSelected(selectedPoint_, selectedSegment_);refreshEditActions();
}

void SketchEditorDialog::setSelectedSegment(int index) {
    selectedSegment_ = index;
    selectedPoint_ = -1;
    refresh();
}
void SketchEditorDialog::setSelectedPoint(int index) {
    if (index < 0) { selectedPoint_ = -1; selectedSegment_ = -1; refresh(); return; }
    if (index < 0 || static_cast<std::size_t>(index) >= sketch_.points().size()) return;
    selectedPoint_ = index;
    selectedSegment_ = -1;
    x_->setValue(sketch_.points()[static_cast<std::size_t>(index)].xMm);
    y_->setValue(sketch_.points()[static_cast<std::size_t>(index)].yMm);
    refresh();
}
void SketchEditorDialog::addPoint() {
    if (!sketch_.addPoint({x_->value(), y_->value()})) {
        status_->setText(tr("The point was not added: open the profile and use distinct consecutive points."));
        return;
    }
    emit sketchChanged();
    refresh();
}
void SketchEditorDialog::movePoint() {
    if (selectedPoint_ < 0 ||
        !sketch_.movePoint(static_cast<std::size_t>(selectedPoint_), {x_->value(), y_->value()})) {
        status_->setText(tr("Those coordinates would create an invalid segment. The original vertex was retained."));
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
    if (sketch_.arc(index)) {
        changeRadius();
        return;
    }
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

void SketchEditorDialog::fillet() {
    if (selectedPoint_ < 0) {
        QMessageBox::information(this, tr("Round vertex"), tr("Select a corner vertex between two straight segments first."));
        return;
    }
    const auto range=sketch_.filletRadiusRange(static_cast<std::size_t>(selectedPoint_));if(!range){status_->setText(tr("Choose a corner between two straight segments that can support a tangent fillet."));return;}
    bool ok = false;
    const double radius = QInputDialog::getDouble(this, tr("Tangent rounding"),tr("Radius (mm) · allowed %1 to %2").arg(range->first,0,'g',6).arg(range->second,0,'g',6),std::clamp(10.0,range->first,range->second),range->first,range->second,6,&ok);
    if (!ok) return;
    const auto corner = static_cast<std::size_t>(selectedPoint_);
    if (!sketch_.filletVertex(corner, radius)) {
        QMessageBox::warning(this, tr("Rounding failed"),
            tr("The radius must fit both adjacent straight segments. Endpoints, straight vertices and existing arcs cannot be rounded."));
        return;
    }
    selectedSegment_ = selectedPoint_;
    selectedPoint_ = -1;
    emit sketchChanged();
    refresh();
}

void SketchEditorDialog::changeRadius() {
    const auto index = static_cast<std::size_t>(selectedSegment_);
    const auto c = sketch_.arc(index);
    if (!c) {
        QMessageBox::information(this, tr("Arc radius"), tr("Select a tangent rounding arc first."));
        return;
    }
    const auto range=sketch_.arcFilletRadiusRange(index);if(!range){status_->setText(tr("This arc is not an editable tangent fillet. Select a supported fillet arc."));return;}
    bool ok = false;
    const double radius = QInputDialog::getDouble(this,tr("Arc radius"),tr("Radius (mm) · allowed %1 to %2").arg(range->first,0,'g',6).arg(range->second,0,'g',6),std::clamp(c->radiusMm,range->first,range->second),range->first,range->second,6,&ok);
    if (!ok) return;
    if (!sketch_.setFilletRadius(index, radius)) {
        QMessageBox::warning(this, tr("Rounding failed"),
            tr("This radius does not fit the tangent lines, or the selected arc has been split. The sketch was not changed."));
        return;
    }
    emit sketchChanged();
    refresh();
}
void SketchEditorDialog::setSketch(const Sketch2D& shape) {
    sketch_ = shape;history_={};committed_={sketch_,nozzleDims_};dragEditing_=false;dragRecorded_=false;
    canvas_->setSketch(&sketch_);
    if (auto* select = findChild<QPushButton*>(QStringLiteral("vortia-sketch-select"))) select->click();
    selectedPoint_ = -1;
    selectedSegment_ = -1;
    refresh();
}

void SketchEditorDialog::setReadOnly(bool readOnly) {
    readOnly_ = readOnly;
    canvas_->setReadOnly(readOnly);
    if (readOnly) findChild<QPushButton*>(QStringLiteral("vortia-sketch-select"))->click();
    for (const auto* id : {"draw", "rectangle", "circle", "nozzle", "add", "move", "split",
                          "dimension", "fillet", "radius", "close-profile", "export", "load", "clear"})
        if (auto* button = findChild<QPushButton*>(QStringLiteral("vortia-sketch-")+id)) button->setEnabled(!readOnly);
    table_->setEditTriggers(readOnly?QAbstractItemView::NoEditTriggers:QAbstractItemView::DoubleClicked|QAbstractItemView::EditKeyPressed);
    x_->setReadOnly(readOnly); y_->setReadOnly(readOnly);refresh();
    if (findChild<QPushButton*>(QStringLiteral("vortia-sketch-select"))->isChecked())
        findChild<QLabel*>(QStringLiteral("vortia-sketch-mode-hint"))->setText(readOnly
            ? tr("Case geometry: select to inspect; drag empty space to pan.")
            : tr("Select: drag vertices or empty space."));
    findChild<QLabel*>(QStringLiteral("vortia-sketch-intro"))->setText(readOnly
        ? tr("OpenFOAM case geometry: pinch to zoom, pan and inspect dimensions. Open the editable source project to change geometry and generate a new mesh revision.")
        : tr("Precise 2D sketch in millimetres. Select a segment to split it or enter an exact length. "
             "This sketch is not yet a CFD-ready mesh."));
}

bool SketchEditorDialog::writeSketchFile(const QString& path) const {
    QJsonObject root{{QStringLiteral("schemaVersion"), sketch_.hasArcs() ? 2 : 1},
                     {QStringLiteral("units"), QStringLiteral("mm")},
                     {QStringLiteral("closed"), sketch_.closed()}};
    QJsonArray vertices;
    for (const auto& p : sketch_.points())
        vertices.append(QJsonArray{p.xMm, p.yMm});
    root.insert(QStringLiteral("vertices"), vertices);
    if (sketch_.hasArcs()) {
        QJsonArray bulges;
        for (std::size_t i = 0; i < sketch_.segmentCount(); ++i) bulges.append(sketch_.segmentBulge(i));
        root.insert(QStringLiteral("segmentBulges"), bulges);
    }
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
    const auto contents = QJsonDocument(root).toJson(QJsonDocument::Indented);
    return file.open(QIODevice::WriteOnly)
        && file.write(contents) == contents.size()
        && file.commit();
}

void SketchEditorDialog::setToolPanelVisible(bool visible) {
    if(auto* panel=findChild<QWidget*>("vortia-sketch-inspector"))panel->setVisible(visible);
}

void SketchEditorDialog::setEmbedded(bool embedded) {
    for(const auto* name:{"vortia-sketch-intro","vortia-sketch-load","vortia-sketch-save","vortia-sketch-dismiss","vortia-sketch-export","vortia-sketch-export-wedge"})
        if(auto* widget=findChild<QWidget*>(name))widget->setVisible(!embedded);
}

void SketchEditorDialog::setInteractionPreferences(bool snap,double spacing,bool grid,bool dimensions,double zoom) {
    canvas_->setInteractionPreferences(snap,spacing,grid,dimensions,zoom);
    const double step=snap&&std::isfinite(spacing)?std::clamp(spacing,.001,1000.0):1;
    x_->setSingleStep(step);y_->setSingleStep(step);
}

bool SketchEditorDialog::readSketchFile(const QString& path) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly) || file.size() > 4 * 1024 * 1024)
        return false;
    QJsonParseError parse;
    const QJsonDocument document = QJsonDocument::fromJson(file.readAll(), &parse);
    const QJsonObject root = document.object();
    const double version = root.value(QStringLiteral("schemaVersion")).toDouble(-1);
    if (parse.error != QJsonParseError::NoError || !document.isObject() ||
        !root.value(QStringLiteral("schemaVersion")).isDouble() ||
        (version != 1.0 && version != 2.0) ||
        root.value(QStringLiteral("units")).toString() != QStringLiteral("mm") ||
        !root.value(QStringLiteral("vertices")).isArray() ||
        !root.value(QStringLiteral("closed")).isBool())
        return false;
    Sketch2D next;
    const QJsonArray vertices = root.value(QStringLiteral("vertices")).toArray();
    if (vertices.size() > 10000)
        return false;
    for (const auto& row : vertices) {
        const QJsonArray coords = row.toArray();
        if (coords.size() != 2 || !coords[0].isDouble() || !coords[1].isDouble()
            // Covers the UI coordinate range and the sum of both maximum
            // nozzle lengths. Extreme imported doubles cannot fit the canvas.
            || std::abs(coords[0].toDouble()) > 2000000.0
            || std::abs(coords[1].toDouble()) > 2000000.0
            || !next.addPoint({coords[0].toDouble(), coords[1].toDouble()}))
            return false;
    }
    if (root.value(QStringLiteral("closed")).toBool() && !next.close())
        return false;
    if (version == 2.0) {
        const auto value = root.value(QStringLiteral("segmentBulges"));
        if (!value.isArray() || static_cast<std::size_t>(value.toArray().size()) != next.segmentCount()) return false;
        const auto bulges = value.toArray();
        for (qsizetype i = 0; i < bulges.size(); ++i)
            if (!bulges[i].isDouble() || !next.setSegmentBulge(static_cast<std::size_t>(i), bulges[i].toDouble())) return false;
    } else if (root.contains(QStringLiteral("segmentBulges"))) {
        return false;
    }
    NozzleDimensions nextDimensions;
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
        nextDimensions = loaded;
    }
    nozzleDims_ = nextDimensions;
    setSketch(next);
    return true;
}

void SketchEditorDialog::exportWedgeMesh() {
    // Never export from stale dimensions after users hand-edit the sketch.
    const auto reference = makeNozzleProfile(nozzleDims_);
    if (!reference || !sketch_.closed() || sketch_.hasArcs() ||
        reference->points().size() != sketch_.points().size()) {
        QMessageBox::warning(this, tr("Wedge mesh"),
            tr("A wedge mesh currently requires an unmodified parametric nozzle."));
        return;
    }
    for(std::size_t i=0;i<reference->points().size();++i) {
        const auto& a=reference->points()[i];
        const auto& b=sketch_.points()[i];
        if(std::hypot(a.xMm-b.xMm,a.yMm-b.yMm)>1e-6) {
            QMessageBox::warning(this, tr("Wedge mesh"),
                tr("Recreate the parametric nozzle before exporting its wedge mesh."));
            return;
        }
    }
    const auto values=dimensionDialog(this,tr("Axisymmetric mesh options"),
        {{tr("Converging segments"),18.0},
         {tr("Diverging segments"),30.0},
         {tr("Cells per segment"),3.0},
         {tr("Radial cells"),40.0},
         {tr("Radial grading"),2.0},
         {tr("Wedge angle (degrees)"),5.0}});
    if(!values) return;
    const AxisymmetricNozzleMeshOptions opts{
        static_cast<int>((*values)[0]),static_cast<int>((*values)[1]),
        static_cast<int>((*values)[2]),static_cast<int>((*values)[3]),
        (*values)[4],(*values)[5]};
    const auto dict=AxisymmetricNozzleBlockMesh::generate(nozzleDims_,opts);
    if(!dict) {
        QMessageBox::warning(this,tr("Wedge mesh"),
                             QString::fromStdString(dict.error().describe()));
        return;
    }
    const QString path=QFileDialog::getSaveFileName(
        this,tr("Export wedge blockMeshDict"),QStringLiteral("blockMeshDict"),
        tr("OpenFOAM dictionary (blockMeshDict)"));
    if(path.isEmpty()) return;
    QSaveFile file(path);
    if(!file.open(QIODevice::WriteOnly) ||
       file.write(QByteArray::fromStdString(dict.value()))<0 ||
       !file.commit()) {
        QMessageBox::warning(this,tr("Mesh export failed"),tr("Could not write blockMeshDict."));
        return;
    }
    QMessageBox::information(this,tr("Wedge mesh exported"),
        tr("The axisymmetric mesh dictionary is exported. Run blockMesh and checkMesh "
           "in an appropriate OpenFOAM case before using it. This is NOT a "
           "complete compressible nozzle simulation."));
}

void SketchEditorDialog::exportNozzleMesh() {
    const auto reference = makeNozzleProfile(nozzleDims_);
    if (!reference || !sketch_.closed() || sketch_.hasArcs() ||
        reference->points().size() != sketch_.points().size()) {
        QMessageBox::warning(this, tr("Nozzle mesh"),
            tr("This export currently supports unmodified parametric nozzle profiles only. "
               "Regenerate the nozzle if you edited individual vertices."));
        return;
    }
    for (std::size_t i = 0; i < sketch_.points().size(); ++i) {
        const auto& a = sketch_.points()[i];
        const auto& b = reference->points()[i];
        if (std::hypot(a.xMm - b.xMm, a.yMm - b.yMm) > 1e-6) {
            QMessageBox::warning(this, tr("Nozzle mesh"),
                tr("The edited profile is not identical to the parametric nozzle. "
                   "Exporting its old dimensions would produce the wrong mesh."));
            return;
        }
    }
    const auto opts = dimensionDialog(this, tr("Planar mesh options"),
        {{tr("Axial segments (converging)"), 18.0},
         {tr("Axial segments (diverging)"), 30.0},
         {tr("Cells per axial segment"), 3.0},
         {tr("Radial cells"), 40.0},
         {tr("Radial grading"), 2.0},
         {tr("Thickness (mm)"), 1.0}});
    if (!opts) return;
    const PlanarNozzleMeshOptions params{
        static_cast<int>((*opts)[0]),static_cast<int>((*opts)[1]),
        static_cast<int>((*opts)[2]),static_cast<int>((*opts)[3]),
        (*opts)[4],(*opts)[5]
    };
    const auto mesh = PlanarNozzleBlockMesh::generate(nozzleDims_, params);
    if (!mesh) {
        QMessageBox::warning(this, tr("Nozzle mesh"),
            QString::fromStdString(mesh.error().describe()));
        return;
    }
    const QString filename = QFileDialog::getSaveFileName(
        this, tr("Export OpenFOAM blockMeshDict"),
        QStringLiteral("blockMeshDict"), tr("OpenFOAM dictionary (blockMeshDict)"));
    if (filename.isEmpty()) return;
    QSaveFile file(filename);
    if (!file.open(QIODevice::WriteOnly) ||
        file.write(QByteArray::fromStdString(mesh.value())) < 0 || !file.commit()) {
        QMessageBox::warning(this, tr("Mesh export failed"),
                             tr("Could not write blockMeshDict."));
        return;
    }
    QMessageBox::information(this, tr("Planar mesh exported"),
        tr("Exported a planar 2D mesh dictionary. Validate it with blockMesh "
           "and checkMesh. This is NOT an axisymmetric wedge or a complete "
           "compressible nozzle CFD case."));
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
    if (!confirmReplacement()) return;
    const auto previous=committed_;const auto history=history_;
    if (!readSketchFile(filename))
        QMessageBox::warning(this, tr("Load error"), tr("Invalid or unsupported sketch format."));
    else {history_=history;committed_=previous;emit sketchChanged();}
}

void SketchEditorDialog::reject() {
    if (!isWindow())
        emit closeRequested();
    else
        QDialog::reject();
}

} // namespace vortia

namespace vortia {
namespace {
bool sameEdit(const SketchEditorDialog::EditSnapshot& a,const SketchEditorDialog::EditSnapshot& b){
    if(a.sketch.closed()!=b.sketch.closed()||a.sketch.points().size()!=b.sketch.points().size())return false;
    for(std::size_t i=0;i<a.sketch.points().size();++i)if(a.sketch.points()[i].xMm!=b.sketch.points()[i].xMm||a.sketch.points()[i].yMm!=b.sketch.points()[i].yMm)return false;
    for(std::size_t i=0;i<a.sketch.segmentCount();++i)if(a.sketch.segmentBulge(i)!=b.sketch.segmentBulge(i))return false;
    const auto& x=a.dimensions;const auto& y=b.dimensions;
    return x.inletDiameterMm==y.inletDiameterMm&&x.throatDiameterMm==y.throatDiameterMm&&x.outletDiameterMm==y.outletDiameterMm&&x.convergingLengthMm==y.convergingLengthMm&&x.divergingLengthMm==y.divergingLengthMm;
}
constexpr const char* sketchMime="application/x-vortia-sketch+json";
std::optional<Sketch2D> clipboardSketch(){
    const auto* mime=QApplication::clipboard()->mimeData();if(!mime)return {};
    const auto bytes=mime->hasFormat(sketchMime)?mime->data(sketchMime):mime->text().toUtf8();
    if(bytes.size()>2*1024*1024)return {};
    const auto doc=QJsonDocument::fromJson(bytes);const auto root=doc.object();
    if(!doc.isObject()||root.value("schemaVersion").toInt()!=2||root.value("units").toString()!="mm"||!root.value("closed").isBool()||!root.value("vertices").isArray()||!root.value("segmentBulges").isArray())return {};
    const auto vertices=root.value("vertices").toArray();if(vertices.isEmpty()||vertices.size()>10000)return {};
    Sketch2D next;
    for(const auto& row:vertices){const auto point=row.toArray();if(point.size()!=2||!point[0].isDouble()||!point[1].isDouble()||std::abs(point[0].toDouble())>2000000||std::abs(point[1].toDouble())>2000000||!next.addPoint({point[0].toDouble(),point[1].toDouble()}))return {};}
    if(root.value("closed").toBool()&&!next.close())return {};
    const auto bulges=root.value("segmentBulges").toArray();if(static_cast<std::size_t>(bulges.size())!=next.segmentCount())return {};
    for(qsizetype i=0;i<bulges.size();++i)if(!bulges[i].isDouble()||!next.setSegmentBulge(static_cast<std::size_t>(i),bulges[i].toDouble()))return {};
    return next;
}
}
void SketchEditorDialog::setEditHistory(EditHistory history){history_=std::move(history);committed_={sketch_,nozzleDims_};refreshEditActions();}
void SketchEditorDialog::recordEdit(){
    const EditSnapshot now{sketch_,nozzleDims_};if(restoring_||sameEdit(committed_,now))return;
    if(!dragEditing_||!dragRecorded_){history_.undo.push_back(committed_);++history_.revision;if(history_.undo.size()>64)history_.undo.erase(history_.undo.begin());history_.redo.clear();dragRecorded_=dragEditing_;}
    committed_=now;refreshEditActions();
}
void SketchEditorDialog::refreshEditActions(){
    if(!undoAction_)return;
    undoAction_->setEnabled(!readOnly_&&!history_.undo.empty());redoAction_->setEnabled(!readOnly_&&!history_.redo.empty());
    copyAction_->setEnabled(!sketch_.points().empty());const auto* mime=QApplication::clipboard()->mimeData();pasteAction_->setEnabled(!readOnly_&&mime&&(mime->hasFormat(sketchMime)||mime->hasText()));deleteAction_->setEnabled(!readOnly_&&selectedPoint_>=0);
}
void SketchEditorDialog::applySnapshot(const EditSnapshot& snapshot){
    restoring_=true;sketch_=snapshot.sketch;nozzleDims_=snapshot.dimensions;committed_=snapshot;dragEditing_=false;dragRecorded_=false;
    selectedPoint_=selectedSegment_=-1;canvas_->setSelected(-1,-1);canvas_->update();refresh();emit sketchChanged();restoring_=false;
}
void SketchEditorDialog::undoEdit(){if(readOnly_||history_.undo.empty())return;history_.redo.push_back(committed_);const auto next=history_.undo.back();history_.undo.pop_back();++history_.revision;applySnapshot(next);}
void SketchEditorDialog::redoEdit(){if(readOnly_||history_.redo.empty())return;history_.undo.push_back(committed_);const auto next=history_.redo.back();history_.redo.pop_back();++history_.revision;applySnapshot(next);}
void SketchEditorDialog::deleteSelectedPoint(){
    if(readOnly_||selectedPoint_<0)return;
    if(!sketch_.removePoint(static_cast<std::size_t>(selectedPoint_))){status_->setText(tr("The vertex cannot be removed without creating a zero-length segment."));return;}
    selectedPoint_=selectedSegment_=-1;emit sketchChanged();refresh();
}
bool SketchEditorDialog::eventFilter(QObject* watched,QEvent* event){
    if(watched==table_&&event->type()==QEvent::KeyPress){const auto* key=static_cast<QKeyEvent*>(event);if(key->key()==Qt::Key_Delete||key->key()==Qt::Key_Backspace){deleteSelectedPoint();return true;}}
    return QDialog::eventFilter(watched,event);
}
void SketchEditorDialog::copySelection(){
    if(sketch_.points().empty())return;Sketch2D fragment;
    if(selectedPoint_>=0){if(!fragment.addPoint(sketch_.points()[static_cast<std::size_t>(selectedPoint_)]))return;}
    else if(selectedSegment_>=0){const auto i=static_cast<std::size_t>(selectedSegment_);if(!fragment.addPoint(sketch_.points()[i])||!fragment.addPoint(sketch_.points()[(i+1)%sketch_.points().size()])||!fragment.setSegmentBulge(0,sketch_.segmentBulge(i)))return;}
    else fragment=sketch_;
    QJsonArray vertices,bulges;for(const auto& point:fragment.points())vertices.append(QJsonArray{point.xMm,point.yMm});for(std::size_t i=0;i<fragment.segmentCount();++i)bulges.append(fragment.segmentBulge(i));
    const auto bytes=QJsonDocument(QJsonObject{{"schemaVersion",2},{"units","mm"},{"closed",fragment.closed()},{"vertices",vertices},{"segmentBulges",bulges}}).toJson(QJsonDocument::Compact);
    auto* mime=new QMimeData;mime->setData(sketchMime,bytes);mime->setText(QString::fromUtf8(bytes));QApplication::clipboard()->setMimeData(mime);
    status_->setText(tr("Copied %1 vertices. With no selection, Copy copies the whole sketch.").arg(fragment.points().size()));
}
void SketchEditorDialog::pasteGeometry(){
    if(readOnly_)return;const auto fragment=clipboardSketch();if(!fragment){status_->setText(tr("The clipboard does not contain valid Vortia geometry."));return;}
    if(sketch_.closed()||(!sketch_.points().empty()&&fragment->closed())){status_->setText(tr("Paste a closed profile into an empty sketch. Open profiles accept appended points or segments; separate contours are not supported."));return;}
    QDialog dialog(this);dialog.setObjectName("vortia-paste-geometry");dialog.setWindowTitle(tr("Paste geometry"));auto* form=new QFormLayout(&dialog);
    auto* note=new QLabel(tr("Coordinates are in millimetres. In an open profile, the copied fragment is appended and connected to its last point."),&dialog);note->setWordWrap(true);form->addRow(note);
    std::array<QDoubleSpinBox*,2> delta{};
    for(std::size_t i=0;i<2;++i){delta[i]=new QDoubleSpinBox(&dialog);delta[i]->setObjectName(QString("paste-offset-%1").arg(i));delta[i]->setRange(-2000000,2000000);delta[i]->setDecimals(6);delta[i]->setSuffix(" mm");delta[i]->setValue(sketch_.points().empty()?0:10);form->addRow(i==0?tr("X offset"):tr("Y offset"),delta[i]);}
    auto* buttons=new QDialogButtonBox(QDialogButtonBox::Ok|QDialogButtonBox::Cancel,&dialog);form->addRow(buttons);connect(buttons,&QDialogButtonBox::accepted,&dialog,&QDialog::accept);connect(buttons,&QDialogButtonBox::rejected,&dialog,&QDialog::reject);
    if(dialog.exec()!=QDialog::Accepted)return;
    Sketch2D next=sketch_;const auto start=next.points().size();
    for(const auto& point:fragment->points()){const SketchPoint2D placed{point.xMm+delta[0]->value(),point.yMm+delta[1]->value()};if(std::abs(placed.xMm)>2000000||std::abs(placed.yMm)>2000000||!next.addPoint(placed)){status_->setText(tr("Paste would create invalid or duplicate consecutive vertices. The sketch was retained."));return;}}
    if(fragment->closed()&&!next.close())return;
    for(std::size_t i=0;i<fragment->segmentCount();++i)if(!next.setSegmentBulge(start+i,fragment->segmentBulge(i)))return;
    sketch_=std::move(next);selectedSegment_=-1;selectedPoint_=static_cast<int>(start);emit sketchChanged();refresh();
}
}

namespace vortia {
void SketchEditorDialog::editVertexCoordinates(int index){
    if(readOnly_||index<0||static_cast<std::size_t>(index)>=sketch_.points().size())return;setSelectedPoint(index);
    if(sketch_.pointLockedByArcs(static_cast<std::size_t>(index))){status_->setText(tr("This vertex supports a tangent arc. Select the arc and edit its radius."));return;}
    QDialog dialog(this);dialog.setObjectName("vortia-edit-vertex");dialog.setWindowTitle(tr("Edit vertex %1").arg(index));auto* form=new QFormLayout(&dialog);const auto point=sketch_.points()[static_cast<std::size_t>(index)];std::array<QDoubleSpinBox*,2> values{};
    for(std::size_t i=0;i<2;++i){values[i]=new QDoubleSpinBox(&dialog);values[i]->setObjectName(QString("vertex-coordinate-%1").arg(i));values[i]->setRange(-2000000,2000000);values[i]->setDecimals(6);values[i]->setSuffix(" mm");values[i]->setValue(i==0?point.xMm:point.yMm);form->addRow(i==0?tr("X coordinate"):tr("Y coordinate"),values[i]);}
    auto* feedback=new QLabel(&dialog);feedback->setWordWrap(true);form->addRow(feedback);auto* buttons=new QDialogButtonBox(QDialogButtonBox::Ok|QDialogButtonBox::Cancel,&dialog);form->addRow(buttons);
    connect(buttons,&QDialogButtonBox::accepted,&dialog,[&](){auto candidate=sketch_;if(!candidate.movePoint(static_cast<std::size_t>(index),{values[0]->value(),values[1]->value()})){feedback->setText(tr("Those coordinates would create an invalid segment. The original vertex was retained."));return;}sketch_=std::move(candidate);emit sketchChanged();refresh();dialog.accept();});connect(buttons,&QDialogButtonBox::rejected,&dialog,&QDialog::reject);dialog.exec();
}
}
