#include "vortia/geometry/SketchSnapping.hpp"
#include <algorithm>
#include <cmath>
#include <limits>
#include <numbers>
namespace vortia {
SketchSnapResult snapSketchPoint(const Sketch2D& s,SketchPoint2D cursor,double tolerance,const SketchSnapOptions& o) {
    SketchSnapResult best{cursor,SketchSnapKind::None,{}};
    if(!o.enabled||!std::isfinite(cursor.xMm)||!std::isfinite(cursor.yMm)||!std::isfinite(tolerance)||tolerance<=0)return best;
    double distance=tolerance;int priority=100;
    const auto offer=[&](SketchPoint2D p,SketchSnapKind kind,int rank,std::vector<SketchPoint2D> refs) {
        const double d=std::hypot(p.xMm-cursor.xMm,p.yMm-cursor.yMm);
        if(d<=tolerance&&(rank<priority||(rank==priority&&d<distance))){best={p,kind,std::move(refs)};distance=d;priority=rank;}
    };
    const auto& points=s.points();
    if(o.endpoints)for(const auto& p:points)offer(p,SketchSnapKind::Endpoint,0,{p});
    for(std::size_t i=0;i<s.segmentCount();++i){
        if(o.midpoints){const auto p=s.segmentPoint(i,.5);offer(p,SketchSnapKind::Midpoint,1,{p});}
        if(o.centers)if(const auto arc=s.arc(i))offer(arc->center,SketchSnapKind::Center,2,{arc->center});
    }
    if(o.alignment&&!points.empty()){
        double dx=tolerance,dy=tolerance;const SketchPoint2D* px=nullptr;const SketchPoint2D* py=nullptr;
        for(const auto& p:points){const double x=std::abs(cursor.xMm-p.xMm),y=std::abs(cursor.yMm-p.yMm);if(x<dx){dx=x;px=&p;}if(y<dy){dy=y;py=&p;}}
        SketchPoint2D p=cursor;std::vector<SketchPoint2D> refs;
        if(px){p.xMm=px->xMm;refs.push_back(*px);}if(py){p.yMm=py->yMm;refs.push_back(*py);}
        if(!refs.empty())offer(p,SketchSnapKind::Alignment,4,refs);
    }
    if(points.empty())return best;
    const auto origin=points.back();
    if(o.perpendicular&&points.size()>1){
        const auto a=points[points.size()-2];const double x=origin.xMm-a.xMm,y=origin.yMm-a.yMm,len=std::hypot(x,y);
        if(len>1e-9){const double t=((cursor.xMm-origin.xMm)*(-y)+(cursor.yMm-origin.yMm)*x)/(len*len);
            offer({origin.xMm-t*y,origin.yMm+t*x},SketchSnapKind::Perpendicular,3,{a,origin});}
        for(std::size_t i=0;i<s.segmentCount();++i)if(!s.arc(i)){
            const auto p=points[i],q=points[(i+1)%points.size()];const double vx=q.xMm-p.xMm,vy=q.yMm-p.yMm,l2=vx*vx+vy*vy;
            if(l2<=1e-18)continue;const double t=((origin.xMm-p.xMm)*vx+(origin.yMm-p.yMm)*vy)/l2;
            if(t>=0&&t<=1){const SketchPoint2D foot{p.xMm+t*vx,p.yMm+t*vy};if(std::hypot(foot.xMm-origin.xMm,foot.yMm-origin.yMm)>1e-6)offer(foot,SketchSnapKind::Perpendicular,3,{p,q,origin});}
        }
    }
    if(o.orthogonal&&priority>2){
        const double dx=cursor.xMm-origin.xMm,dy=cursor.yMm-origin.yMm;
        return {{std::abs(dx)>=std::abs(dy)?cursor.xMm:origin.xMm,std::abs(dx)>=std::abs(dy)?origin.yMm:cursor.yMm},SketchSnapKind::Orthogonal,{origin}};
    }
    if(o.polar&&std::isfinite(o.polarDegrees)&&o.polarDegrees>=1&&o.polarDegrees<=90){
        const double dx=cursor.xMm-origin.xMm,dy=cursor.yMm-origin.yMm,step=o.polarDegrees*std::numbers::pi/180;
        const double angle=std::round(std::atan2(dy,dx)/step)*step,t=dx*std::cos(angle)+dy*std::sin(angle);
        offer({origin.xMm+t*std::cos(angle),origin.yMm+t*std::sin(angle)},SketchSnapKind::Polar,5,{origin});
    }
    return best;
}
}
