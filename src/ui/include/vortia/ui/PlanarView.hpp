#pragma once
#include <QWidget>
#include <QTransform>
namespace vortia {
class PlanarView : public QWidget {
    Q_OBJECT
public:
    explicit PlanarView(QWidget* parent=nullptr);
    void setWorldBounds(QRectF bounds){bounds_=bounds;update();}
    QTransform viewTransform()const;
    void fitView();
signals:
    void worldClicked(QPointF point);
protected:
    bool navigating()const{return pressed_;}
    void mousePressEvent(QMouseEvent*)override;
    void mouseMoveEvent(QMouseEvent*)override;
    void mouseReleaseEvent(QMouseEvent*)override;
    void mouseDoubleClickEvent(QMouseEvent*)override;
    void wheelEvent(QWheelEvent*)override;
    void keyPressEvent(QKeyEvent*)override;
    bool event(QEvent*)override;
private:
    void zoomAt(double factor,QPointF anchor);
    QRectF bounds_;
    double zoom_=1;
    QPointF pan_,press_,previous_;
    bool pressed_=false,dragged_=false;
};
}
