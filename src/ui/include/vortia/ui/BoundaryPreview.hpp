#pragma once
#include "vortia/geometry/Sketch2D.hpp"
#include "vortia/ui/PlanarView.hpp"
#include <QTransform>
class QTableWidget;
namespace vortia {
// Navigation changes only this preview's camera, never geometry or conditions.
class BoundaryPreview final : public PlanarView {
public:
    BoundaryPreview(const Sketch2D& shape, QTableWidget* table, QWidget* parent=nullptr);
protected:
    void paintEvent(QPaintEvent*) override;
    void mouseMoveEvent(QMouseEvent*) override;
private:
    int hitTest(QPointF at) const;
    int pieces(std::size_t segment) const;
    const Sketch2D& shape_;
    QTableWidget* table_;
};
}
