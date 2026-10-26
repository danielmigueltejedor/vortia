#include "vortia/ui/PlanarView.hpp"
#include <QApplication>
#include <QMouseEvent>
#include <QNativeGestureEvent>
#include <QWheelEvent>
#include <QKeyEvent>
#include <QLineF>
#include <algorithm>
#include <cmath>
namespace vortia {
PlanarView::PlanarView(QWidget* parent):QWidget(parent){setFocusPolicy(Qt::StrongFocus);setMouseTracking(true);}
QTransform PlanarView::viewTransform() const {
    const double scale=std::max(1e-9,std::min(std::max(1,width()-64)/std::max(1e-6,bounds_.width()),std::max(1,height()-52)/std::max(1e-6,bounds_.height())))*zoom_;
    QTransform transform;transform.translate(width()/2.0+pan_.x(),height()/2.0+pan_.y());transform.scale(scale,-scale);transform.translate(-bounds_.center().x(),-bounds_.center().y());return transform;
}
void PlanarView::fitView(){zoom_=1;pan_={};pressed_=dragged_=false;unsetCursor();update();}
void PlanarView::mousePressEvent(QMouseEvent* event){
    if(event->button()!=Qt::LeftButton&&event->button()!=Qt::MiddleButton){QWidget::mousePressEvent(event);return;}
    setFocus(Qt::MouseFocusReason);pressed_=true;dragged_=event->button()==Qt::MiddleButton;press_=previous_=event->position();event->accept();
}
void PlanarView::mouseMoveEvent(QMouseEvent* event){
    if(pressed_&&(event->buttons()&(Qt::LeftButton|Qt::MiddleButton))){
        if(!dragged_&&QLineF(press_,event->position()).length()>=QApplication::startDragDistance())dragged_=true;
        if(dragged_){pan_+=event->position()-previous_;setCursor(Qt::ClosedHandCursor);update();}previous_=event->position();event->accept();return;
    }
    setCursor(Qt::OpenHandCursor);

}
void PlanarView::mouseReleaseEvent(QMouseEvent* event){
    if(!pressed_){QWidget::mouseReleaseEvent(event);return;}
    const bool select=event->button()==Qt::LeftButton&&!dragged_;pressed_=dragged_=false;unsetCursor();
    if(select)emit worldClicked(viewTransform().inverted().map(event->position()));event->accept();
}
void PlanarView::mouseDoubleClickEvent(QMouseEvent* event){if(event->button()==Qt::LeftButton){fitView();event->accept();}else QWidget::mouseDoubleClickEvent(event);}
void PlanarView::zoomAt(double factor,QPointF anchor){
    if(!std::isfinite(factor)||factor<=0)return;const double next=std::clamp(zoom_*std::clamp(factor,.2,5.0),.05,10000.0);const auto relative=anchor-QPointF(width()/2.0,height()/2.0);pan_=relative-(relative-pan_)*(next/zoom_);zoom_=next;update();
}
void PlanarView::wheelEvent(QWheelEvent* event){
    if(!event->pixelDelta().isNull()&&!(event->modifiers()&(Qt::ControlModifier|Qt::MetaModifier))){pan_+=event->pixelDelta();update();}
    else {const double delta=event->angleDelta().y()!=0?event->angleDelta().y():event->pixelDelta().y();zoomAt(std::pow(1.2,std::clamp(delta/120.0,-5.0,5.0)),event->position());}event->accept();
}
bool PlanarView::event(QEvent* event){
    if(event->type()==QEvent::NativeGesture){auto* gesture=static_cast<QNativeGestureEvent*>(event);
        if(gesture->gestureType()==Qt::ZoomNativeGesture){zoomAt(1+gesture->value(),gesture->position());event->accept();return true;}
        if(gesture->gestureType()==Qt::PanNativeGesture){pan_+=gesture->delta();update();event->accept();return true;}
    }
    if(event->type()==QEvent::FocusOut||event->type()==QEvent::WindowDeactivate){pressed_=dragged_=false;unsetCursor();}
    return QWidget::event(event);
}
void PlanarView::keyPressEvent(QKeyEvent* event){if(event->key()==Qt::Key_F){fitView();event->accept();}else QWidget::keyPressEvent(event);}
}
