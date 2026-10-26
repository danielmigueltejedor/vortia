#pragma once
#include "vortia/geometry/Sketch2D.hpp"
#include <QWidget>
#include <QTransform>
class QTableWidget;
namespace vortia {
// Navigation changes only this preview's camera, never geometry or conditions.
class BoundaryPreview final : public QWidget {
public:
    BoundaryPreview(const Sketch2D& shape, QTableWidget* table, QWidget* parent=nullptr);
    void fitView();
    QTransform viewTransform() const;
protected:
    void paintEvent(QPaintEvent*) override;
    void mousePressEvent(QMouseEvent*) override;
    void mouseMoveEvent(QMouseEvent*) override;
    void mouseReleaseEvent(QMouseEvent*) override;
    void mouseDoubleClickEvent(QMouseEvent*) override;
    void wheelEvent(QWheelEvent*) override;
    void keyPressEvent(QKeyEvent*) override;
    bool event(QEvent*) override;
private:
    int hitTest(QPointF at) const;
    int pieces(std::size_t segment) const;
    void zoomAt(double factor, QPointF anchor);
    const Sketch2D& shape_;
    QTableWidget* table_;
    QRectF bounds_;
    double zoom_=1;
    QPointF pan_,press_,previous_;
    bool pressed_=false,dragged_=false;
};
}
