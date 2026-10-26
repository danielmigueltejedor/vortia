#include "vortia/ui/BoundaryMapping.hpp"
#include <QJsonArray>
#include <QJsonDocument>
#include <QRegularExpression>
#include <QFile>
#include <QDir>
#include <set>
#include <map>
#include <algorithm>
#include <cmath>
#include <numbers>
namespace vortia {
namespace {
std::optional<Sketch2D> readGeometry(const QJsonObject& json){
    if(!json.value("vertices").isArray()||!json.value("closed").isBool())return {};
    Sketch2D s;const auto points=json.value("vertices").toArray();if(points.size()>10000)return {};
    for(const auto& value:points){const auto p=value.toArray();if(p.size()!=2||!p[0].isDouble()||!p[1].isDouble()||!s.addPoint({p[0].toDouble(),p[1].toDouble()}))return {};}
    if(json.value("closed").toBool()&&!s.close())return {};
    const auto curves=json.value("segmentBulges");if(!curves.isUndefined()){
        if(!curves.isArray()||static_cast<std::size_t>(curves.toArray().size())!=s.segmentCount())return {};
        const auto values=curves.toArray();for(qsizetype i=0;i<values.size();++i)if(!values[i].isDouble()||!s.setSegmentBulge(static_cast<std::size_t>(i),values[i].toDouble()))return {};
    }
    return s;
}
bool near(SketchPoint2D a,SketchPoint2D b){return std::hypot(a.xMm-b.xMm,a.yMm-b.yMm)<=1e-6;}
bool onSegment(const Sketch2D& s,std::size_t index,SketchPoint2D point){
    const auto a=s.points()[index],b=s.points()[(index+1)%s.points().size()];const double tol=1e-6;
    if(const auto arc=s.arc(index)){
        if(std::abs(std::hypot(point.xMm-arc->center.xMm,point.yMm-arc->center.yMm)-arc->radiusMm)>tol)return false;
        if(near(a,point)||near(b,point))return true;
        const double angle=std::atan2(point.yMm-arc->center.yMm,point.xMm-arc->center.xMm),tau=2*std::numbers::pi;
        const double delta=arc->sweepRadians>0?std::fmod(angle-arc->startRadians+2*tau,tau):-std::fmod(arc->startRadians-angle+2*tau,tau);
        const double t=delta/arc->sweepRadians;return t>=0&&t<=1;
    }
    const double dx=b.xMm-a.xMm,dy=b.yMm-a.yMm,length=std::hypot(dx,dy);if(length<tol)return false;
    const double t=((point.xMm-a.xMm)*dx+(point.yMm-a.yMm)*dy)/(length*length);
    return t>=-tol/length&&t<=1+tol/length&&std::abs((point.xMm-a.xMm)*dy-(point.yMm-a.yMm)*dx)/length<=tol;
}
bool contained(const Sketch2D& source,std::size_t parent,const Sketch2D& target,std::size_t child){
    for(const double t:{0.0,.25,.5,.75,1.0})if(!onSegment(source,parent,target.segmentPoint(child,t)))return false;return true;
}
QJsonObject defaults(const QString& name,const QString& type){return {{"name",name},{"type",type},{"pressure",101325},{"temperature",300},{"viscosityRatio",10}};}
QJsonObject physical(QJsonObject bc){
    bc.remove("name");if(bc.value("type")=="wall"||bc.value("type")=="axis"){bc.insert("pressure",101325);bc.insert("temperature",300);bc.insert("viscosityRatio",10);}return bc;
}
bool samePhysics(const QJsonObject& a,const QJsonObject& b){return physical(a)==physical(b);}
}
QJsonObject boundaryGeometry(const Sketch2D& shape){
    QJsonArray points,bulges;for(const auto& p:shape.points())points.append(QJsonArray{p.xMm,p.yMm});for(std::size_t i=0;i<shape.segmentCount();++i)bulges.append(shape.segmentBulge(i));return {{"vertices",points},{"segmentBulges",bulges},{"closed",shape.closed()}};
}
QJsonArray normalizedBoundaryNames(const QJsonArray& boundaries){
    QJsonArray result;std::map<QString,QJsonObject> used;
    for(const auto& value:boundaries){auto bc=value.toObject();QString name=bc.value("name").toString().normalized(QString::NormalizationForm_D);name.remove(QRegularExpression("[\\x{0300}-\\x{036f}]"));name.replace(QRegularExpression("[^A-Za-z0-9_]"),"_");name.replace(QRegularExpression("_+"),"_");if(name.isEmpty())name="boundary";if(!name.front().isLetter())name="b_"+name;name=name.left(48);
        if(QStringList{"wedgeFront","wedgeBack","fluid","defaultFaces"}.contains(name))name=(name+"_patch").left(48);
        if(bc.value("type")=="wall"||bc.value("type")=="axis"){bc.insert("pressure",101325);bc.insert("temperature",300);bc.insert("viscosityRatio",10);}
        const auto base=name;int suffix=2;while(used.contains(name)&&!samePhysics(used.at(name),bc)){const auto tail="_"+QString::number(suffix++);name=base.left(48-tail.size())+tail;}
        bc.insert("name",name);used[name]=bc;result.append(bc);
    }
    return result;
}
QJsonObject reconcileFlowBoundaries(const Sketch2D& before,const Sketch2D& after,const QJsonObject& config,bool remember){
    if(config.isEmpty())return config;auto result=config;const auto old=config.value("boundaries").toArray();const auto target=boundaryGeometry(after);const auto original=boundaryGeometry(before);
    auto versions=config.value("boundaryVersions").toArray();
    if(remember&&before.segmentCount()==static_cast<std::size_t>(old.size())&&original!=target){
        QJsonArray retained;for(const auto& version:versions)if(version.toObject().value("geometry").toObject()!=original)retained.append(version);
        retained.append(QJsonObject{{"geometry",original},{"boundaries",old}});while(retained.size()>64)retained.removeFirst();versions=retained;
        result.insert("boundaryVersions",versions);
    }
    for(qsizetype index=versions.size();index>0;--index){const auto version=versions[index-1].toObject();if(version.value("geometry").toObject()==target&&version.value("boundaries").toArray().size()==static_cast<qsizetype>(after.segmentCount())){result.insert("boundaries",normalizedBoundaryNames(version.value("boundaries").toArray()));result.insert("boundaryGeometry",target);result.remove("unmappedBoundaries");return result;}}
    if(original==target&&old.size()==static_cast<qsizetype>(after.segmentCount())){result.insert("boundaryGeometry",target);return result;}
    QJsonArray mapped;std::vector<bool> assigned(after.segmentCount(),false),used(before.segmentCount(),false);
    bool moved=before.points().size()==after.points().size()&&before.closed()==after.closed();int anchors=0;
    if(moved)for(std::size_t i=0;i<before.points().size();++i)anchors+=near(before.points()[i],after.points()[i]);
    moved=moved&&anchors>=std::max(1,static_cast<int>(before.points().size())-2);
    for(std::size_t child=0;child<after.segmentCount();++child){std::vector<std::size_t> parents;
        if(moved&&child<static_cast<std::size_t>(old.size()))parents.push_back(child);
        if(parents.empty()&&old.size()==static_cast<qsizetype>(before.segmentCount()))for(std::size_t parent=0;parent<before.segmentCount();++parent)if(contained(before,parent,after,child)||contained(after,child,before,parent))parents.push_back(parent);
        if(parents.empty()&&moved&&child<static_cast<std::size_t>(old.size()))parents.push_back(child);
        QJsonObject bc;bool compatible=!parents.empty();for(const auto parent:parents){const auto candidate=old[static_cast<qsizetype>(parent)].toObject();if(bc.isEmpty())bc=candidate;else compatible=compatible&&samePhysics(bc,candidate);}
        if(compatible){assigned[child]=true;for(const auto parent:parents)used[parent]=true;}
        else bc=defaults(QString("edge_%1").arg(child+1),"unassigned");
        const auto a=after.points()[child],b=after.points()[(child+1)%after.points().size()];const bool axis=a.yMm==0&&b.yMm==0&&after.segmentBulge(child)==0;
        if(axis){if(bc.value("type")!="axis")bc=defaults("axis","axis");assigned[child]=true;}
        else if(bc.value("type")=="axis"){bc=defaults(QString("edge_%1").arg(child+1),"unassigned");assigned[child]=false;}
        mapped.append(bc);
    }
    // An inserted tangent arc between two already assigned walls is a wall.
    // Different/open physical boundary types are deliberately left unresolved.
    for(std::size_t i=0;i<after.segmentCount();++i)if(!assigned[i]&&after.arc(i)&&after.segmentCount()>2){
        const auto prev=(i+after.segmentCount()-1)%after.segmentCount(),next=(i+1)%after.segmentCount();
        if(!after.closed()&&(i==0||i+1==after.segmentCount()))continue;
        const auto left=mapped[static_cast<qsizetype>(prev)].toObject(),right=mapped[static_cast<qsizetype>(next)].toObject();
        if(assigned[prev]&&assigned[next]&&left.value("type")=="wall"&&right.value("type")=="wall"){
            auto bc=left;if(left.value("name")!=right.value("name"))bc.insert("name",QString("rounded_wall_%1").arg(i+1));mapped[static_cast<qsizetype>(i)]=bc;assigned[i]=true;
        }
    }
    QJsonArray unmapped=config.value("unmappedBoundaries").toArray();for(std::size_t i=0;i<used.size()&&i<static_cast<std::size_t>(old.size());++i)if(!used[i]&&!unmapped.contains(old[static_cast<qsizetype>(i)]))unmapped.append(old[static_cast<qsizetype>(i)]);
    result.insert("boundaries",normalizedBoundaryNames(mapped));result.insert("boundaryGeometry",target);if(unmapped.isEmpty())result.remove("unmappedBoundaries");else result.insert("unmappedBoundaries",unmapped);
    return result;
}
QJsonObject restoreBoundaryGeometry(const Sketch2D& current,const QJsonObject& config){
    if(config.isEmpty()||(current.points().empty()&&config.value("boundaries").toArray().isEmpty()))return config;
    if(const auto previous=readGeometry(config.value("boundaryGeometry").toObject()))return reconcileFlowBoundaries(*previous,current,config);
    if(config.value("boundaries").toArray().size()==static_cast<qsizetype>(current.segmentCount())){auto result=config;result.insert("boundaryGeometry",boundaryGeometry(current));return result;}
    QFile source(QDir(config.value("activeCase").toString()).filePath("geometry/working-sketch.vsketch.json"));
    if(!config.value("activeCase").toString().isEmpty()&&source.open(QIODevice::ReadOnly)&&source.size()<4*1024*1024)if(const auto previous=readGeometry(QJsonDocument::fromJson(source.readAll()).object()))if(previous->segmentCount()==static_cast<std::size_t>(config.value("boundaries").toArray().size()))return reconcileFlowBoundaries(*previous,current,config);
    return reconcileFlowBoundaries({},current,config);
}
}

namespace vortia {
QJsonObject normalizedFlowConfiguration(const Sketch2D& shape,const QJsonObject& input){
    auto result=restoreBoundaryGeometry(shape,input);if(result.isEmpty())return result;
    auto boundaries=normalizedBoundaryNames(result.value("boundaries").toArray());
    for(qsizetype i=0;i<boundaries.size()&&i<static_cast<qsizetype>(shape.segmentCount());++i){auto bc=boundaries[i].toObject();const auto a=shape.points()[static_cast<std::size_t>(i)],b=shape.points()[(static_cast<std::size_t>(i)+1)%shape.points().size()];const bool axis=a.yMm==0&&b.yMm==0&&shape.segmentBulge(static_cast<std::size_t>(i))==0;if(axis)bc.insert("type","axis");else if(bc.value("type")=="axis")bc.insert("type","unassigned");boundaries[i]=bc;}
    result.insert("boundaries",normalizedBoundaryNames(boundaries));auto solver=result.value("solver").toObject();const double iterations=solver.value("iterations").toDouble(-1),save=solver.value("writeInterval").toDouble(-1);if(iterations>=1&&save>iterations)solver.insert("writeInterval",iterations);auto convergence=solver.value("convergence").toObject();const double interval=convergence.value("interval").toDouble(50),minimum=convergence.value("minimumIterations").toDouble(500);if(interval>=1&&interval<=10000&&minimum<interval){convergence.insert("minimumIterations",interval);solver.insert("convergence",convergence);}result.insert("solver",solver);
    auto mesh=result.value("mesh").toObject();const double global=mesh.value("sizeMm").toDouble(-1),wall=mesh.value("wallSizeMm").toDouble(-1);if(global>0&&wall>global){mesh.insert("wallSizeMm",global);result.insert("mesh",mesh);}return result;
}
}
