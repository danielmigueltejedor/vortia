#include "vortia/ui/BoundaryPreview.hpp"
#include <QApplication>
#include <QComboBox>
#include <QCoreApplication>
#include <QKeyEvent>
#include <QLineEdit>
#include <QLineF>
#include <QMouseEvent>
#include <QNativeGestureEvent>
#include <QPainter>
#include <QPainterPath>
#include <QTableWidget>
#include <QToolTip>
#include <QWheelEvent>
#include <algorithm>
#include <cmath>
#include <limits>

namespace vortia {
namespace {
QString previewText(const char* text){return QCoreApplication::translate("vortia::FlowSetup",text);}
}
BoundaryPreview::BoundaryPreview(const Sketch2D& shape,QTableWidget* table,QWidget* parent)
    :PlanarView(parent),shape_(shape),table_(table){
    setObjectName("flow-boundary-preview");setMinimumHeight(190);setMaximumHeight(260);
    setSizePolicy(QSizePolicy::Expanding,QSizePolicy::Expanding);
    setFocusPolicy(Qt::StrongFocus);setMouseTracking(true);
    setToolTip(previewText("Click a numbered segment to select its boundary settings."));
    double minX=std::numeric_limits<double>::infinity(),minY=minX,maxX=-minX,maxY=-minX;
    for(std::size_t i=0;i<shape_.segmentCount();++i)for(int j=0;j<=48;++j){
        const auto v=shape_.segmentPoint(i,j/48.0);
        minX=std::min(minX,v.xMm);maxX=std::max(maxX,v.xMm);minY=std::min(minY,v.yMm);maxY=std::max(maxY,v.yMm);
    }
    if(shape_.segmentCount())setWorldBounds(QRectF(QPointF(minX,minY),QPointF(maxX,maxY)));
    connect(this,&PlanarView::worldClicked,this,[this](QPointF world){const int row=hitTest(viewTransform().map(world));if(row>=0){table_->setCurrentCell(row,0);table_->selectRow(row);table_->scrollToItem(table_->item(row,0));update();}});
}
int BoundaryPreview::pieces(std::size_t segment)const{
    const auto arc=shape_.arc(segment);if(!arc)return 1;
    const double radius=arc->radiusMm*std::abs(viewTransform().m11());
    const double step=2*std::acos(std::clamp(1-.4/std::max(.4,radius),-1.0,1.0));
    return std::clamp(static_cast<int>(std::ceil(std::abs(arc->sweepRadians)/std::max(1e-6,step))),4,4096);
}
void BoundaryPreview::paintEvent(QPaintEvent*){
    QPainter painter(this);painter.setRenderHint(QPainter::Antialiasing);painter.fillRect(rect(),palette().color(QPalette::Base));
    const auto transform=viewTransform();
    for(std::size_t i=0;i<shape_.segmentCount();++i){
        const int row=static_cast<int>(i);auto* editor=qobject_cast<QComboBox*>(table_->cellWidget(row,2));const auto kind=editor?editor->currentData().toString():QString{};
        const QColor color=kind=="inlet"?QColor("#26a87b"):kind=="outlet"?QColor("#e99445"):kind=="reservoir"?QColor("#a683dc"):kind=="axis"?QColor("#5ca7b8"):kind=="wall"?QColor("#3795bd"):QColor("#dc5f68");
        const bool selected=table_->currentRow()==row;QPainterPath path;const int count=pieces(i);
        for(int j=0;j<=count;++j){const auto v=shape_.segmentPoint(i,static_cast<double>(j)/count);const auto at=transform.map(QPointF(v.xMm,v.yMm));if(j==0)path.moveTo(at);else path.lineTo(at);}
        painter.setPen(QPen(color,selected?4:2,kind=="axis"?Qt::DashLine:Qt::SolidLine));painter.drawPath(path);
        const auto midpoint=shape_.segmentPoint(i,.5);const auto at=transform.map(QPointF(midpoint.xMm,midpoint.yMm));const int labelWidth=std::max(26,painter.fontMetrics().horizontalAdvance(QString::number(row+1))+12);
        QRectF label(at.x()-labelWidth/2.0,at.y()-12,labelWidth,24);painter.setPen(selected?palette().color(QPalette::Text):color);painter.setBrush(palette().color(QPalette::Base));painter.drawRoundedRect(label,5,5);painter.drawText(label,Qt::AlignCenter,QString::number(row+1));
    }
}
int BoundaryPreview::hitTest(QPointF pos)const{
    double best=16;int row=-1;const auto transform=viewTransform();
    for(std::size_t i=0;i<shape_.segmentCount();++i){const int count=pieces(i);
        for(int j=0;j<count;++j){const auto pa=shape_.segmentPoint(i,static_cast<double>(j)/count),pb=shape_.segmentPoint(i,static_cast<double>(j+1)/count);const auto a=transform.map(QPointF(pa.xMm,pa.yMm)),b=transform.map(QPointF(pb.xMm,pb.yMm)),v=b-a;const double denominator=QPointF::dotProduct(v,v);const double t=denominator>0?std::clamp(QPointF::dotProduct(pos-a,v)/denominator,0.0,1.0):0;const double distance=QLineF(pos,a+t*v).length();if(distance<best){best=distance;row=static_cast<int>(i);}}
        const auto midpoint=shape_.segmentPoint(i,.5);if(QLineF(pos,transform.map(QPointF(midpoint.xMm,midpoint.yMm))).length()<14){row=static_cast<int>(i);best=0;}
    }
    return row;
}
void BoundaryPreview::mouseMoveEvent(QMouseEvent* event){
    PlanarView::mouseMoveEvent(event);if(navigating())return;
    const int row=hitTest(event->position());setCursor(row>=0?Qt::PointingHandCursor:Qt::OpenHandCursor);
    if(row>=0){auto* name=qobject_cast<QLineEdit*>(table_->cellWidget(row,1));auto* type=qobject_cast<QComboBox*>(table_->cellWidget(row,2));QToolTip::showText(event->globalPosition().toPoint(),previewText("Segment %1 · %2 · %3").arg(row+1).arg(name?name->text():QString{}).arg(type?type->currentText():QString{}),this);}else QToolTip::hideText();
}
}
