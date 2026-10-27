#include "vortia/visualization/SectionAnalysis.hpp"
#include "vortia/core/Vec3.hpp"
#include <QCryptographicHash>
#include <QJsonDocument>
#include <QtEndian>
#include <vtkCell.h>
#include <vtkCellData.h>
#include <vtkCompositeDataSet.h>
#include <vtkCompositeDataIterator.h>
#include <vtkCutter.h>
#include <vtkDataArray.h>
#include <vtkDataSet.h>
#include <vtkNew.h>
#include <vtkPlane.h>
#include <vtkPointData.h>
#include <vtkPolyData.h>
#include <vtkSmartPointer.h>
#include <vtkTriangleFilter.h>
#include <algorithm>
#include <bit>
#include <cmath>
#include <set>
#include <memory>
#include <map>
#include <stdexcept>
#include <vector>
namespace vortia {
namespace {
void require(bool valid,const char* message){if(!valid)throw std::runtime_error(message);}
std::vector<vtkDataSet*> volumes(vtkDataObject* data) {
    std::vector<vtkDataSet*> result;std::set<vtkDataSet*> seen;
    const auto add=[&](vtkDataObject* item){auto* ds=vtkDataSet::SafeDownCast(item);if(ds&&ds->GetNumberOfCells()&&ds->GetCell(0)->GetCellDimension()==3&&seen.insert(ds).second)result.push_back(ds);};
    add(data);if(auto* composite=vtkCompositeDataSet::SafeDownCast(data)) {
        auto iterator=vtkSmartPointer<vtkCompositeDataIterator>::Take(composite->NewIterator());
        for(iterator->InitTraversal();!iterator->IsDoneWithTraversal();iterator->GoToNextItem())add(iterator->GetCurrentDataObject());
    }
    require(!result.empty(),"Section analysis requires a three-dimensional fluid volume");return result;
}
struct Field {
    vtkDataArray* array{};bool points{};int components{};
    Field(vtkDataSet* data,const char* name,int size):components(size) {
        array=data->GetCellData()->GetArray(name);points=!array;if(!array)array=data->GetPointData()->GetArray(name);
        require(array&&array->GetNumberOfComponents()==size,"Required fields p, U or rho are missing or have incompatible components");
        require(array->GetNumberOfTuples()>=(points?data->GetNumberOfPoints():data->GetNumberOfCells()),"The section contains an incomplete field");
    }
    double value(vtkCell* cell,vtkIdType index,int component,const std::array<double,3>& weights) const {
        double result=0;if(points)for(int j=0;j<3;++j)result+=weights[j]*array->GetComponent(cell->GetPointId(j),component);
        else result=array->GetComponent(index,component);
        require(std::isfinite(result),"The section contains non-finite field values");return result;
    }
};
}
std::array<double,6> sectionAnalysisBounds(vtkDataObject* data) {
    const auto sources=volumes(data);std::array<double,6> result{};sources.front()->GetBounds(result.data());
    for(auto* source:sources){double bounds[6];source->GetBounds(bounds);for(int a=0;a<3;++a){result[2*a]=std::min(result[2*a],bounds[2*a]);result[2*a+1]=std::max(result[2*a+1],bounds[2*a+1]);}}return result;
}
QJsonObject sectionOptionsJson(const SectionOptions& o){return {{"axis",o.axis},{"normal",o.normal},{"position",o.position},{"referencePressure",o.referencePressure},{"constantDensity",o.constantDensity},{"referenceDensity",o.referenceDensity},{"referenceSpeed",o.referenceSpeed},{"referenceArea",o.referenceArea},{"useDensityField",o.useDensityField},{"kinematicPressure",o.kinematicPressure},{"fullAnnulus",o.fullAnnulus},{"wedgeAngle",o.wedgeAngle}};}
SectionOptions sectionOptionsFromJson(const QJsonObject& j){SectionOptions o;
    o.axis=j.value("axis").toInt();o.normal=j.value("normal").toInt(1);o.position=j.value("position").toDouble();o.referencePressure=j.value("referencePressure").toDouble(101325);o.constantDensity=j.value("constantDensity").toDouble(1.225);o.referenceDensity=j.value("referenceDensity").toDouble(1.225);o.referenceSpeed=j.value("referenceSpeed").toDouble(100);o.referenceArea=j.value("referenceArea").toDouble(1);o.useDensityField=j.value("useDensityField").toBool(true);o.kinematicPressure=j.value("kinematicPressure").toBool();o.fullAnnulus=j.value("fullAnnulus").toBool();o.wedgeAngle=j.value("wedgeAngle").toDouble();return o;}
QJsonObject sectionMetricsJson(const SectionAnalysis& r){return {{"area_m2",r.area},{"massFlow_kg_s",r.massFlow},{"forwardMassFlow_kg_s",r.forwardMassFlow},{"reverseMassFlow_kg_s",r.reverseMassFlow},{"volumeFlow_m3_s",r.volumeFlow},{"meanPressure_Pa",r.meanPressure},{"meanDensity_kg_m3",r.meanDensity},{"meanNormalVelocity_m_s",r.meanNormalVelocity},{"momentumFlux_N",r.momentumFlux},{"pressureLoad_N",r.pressureLoad},{"normalResultant_N",r.resultant},{"kineticPower_W",r.kineticPower},{"normalCoefficient",r.coefficient},{"meanCp",r.meanCp}};}
SectionAnalysis analyzeSection(vtkDataObject* data,const SectionOptions& o) {
    require(o.axis>=0&&o.axis<3&&(o.normal==1||o.normal==-1),"Invalid section orientation");
    for(double v:{o.position,o.referencePressure,o.constantDensity,o.referenceDensity,o.referenceSpeed,o.referenceArea,o.wedgeAngle})require(std::isfinite(v),"Section inputs must be finite");
    require(o.constantDensity>0&&o.referenceDensity>0&&o.referenceSpeed>0&&o.referenceArea>0,"Density, reference speed and reference area must be positive");
    require(!o.fullAnnulus||(o.axis==0&&o.wedgeAngle>0&&o.wedgeAngle<=5),"Full-annulus reporting requires a known axisymmetric X wedge of at most five degrees");
    const auto sources=volumes(data);SectionAnalysis result;result.areaScale=o.fullAnnulus?360/o.wedgeAngle:1;
    QCryptographicHash fingerprint(QCryptographicHash::Sha256);fingerprint.addData(QJsonDocument(sectionOptionsJson(o)).toJson(QJsonDocument::Compact));
    const auto hashDouble=[&](double value){const auto bits=qToLittleEndian(std::bit_cast<quint64>(value));fingerprint.addData(QByteArrayView(reinterpret_cast<const char*>(&bits),sizeof bits));};
    // Seven-point, degree-five triangle quadrature integrates products of linear
    // point fields without replacing rho*U or rho*U^2 with products of averages.
    struct Quadrature{std::array<double,3> b;double w;};
    const std::array<Quadrature,7> quadrature{{{{1./3,1./3,1./3},.225},{{.059715871789770,.470142064105115,.470142064105115},.132394152788506},{{.470142064105115,.059715871789770,.470142064105115},.132394152788506},{{.470142064105115,.470142064105115,.059715871789770},.132394152788506},{{.797426985353087,.101286507323456,.101286507323456},.125939180544827},{{.101286507323456,.797426985353087,.101286507323456},.125939180544827},{{.101286507323456,.101286507323456,.797426985353087},.125939180544827}}};
    std::map<std::array<double,9>,std::array<double,15>> triangles;
    long double areaSum=0,mass=0,forward=0,reverse=0,volume=0,pressure=0,density=0,normalVelocity=0,momentum=0,load=0,power=0;
    for(auto* source:sources) {
        double bounds[6];source->GetBounds(bounds);if(o.position<bounds[2*o.axis]||o.position>bounds[2*o.axis+1])continue;
        vtkNew<vtkPlane> plane;double origin[3]={0,0,0},normal[3]={0,0,0};origin[o.axis]=o.position;normal[o.axis]=1;plane->SetOrigin(origin);plane->SetNormal(normal);
        vtkNew<vtkCutter> cutter;cutter->SetInputData(source);cutter->SetCutFunction(plane);cutter->Update();
        vtkNew<vtkTriangleFilter> triangulated;triangulated->SetInputConnection(cutter->GetOutputPort());triangulated->PassLinesOff();triangulated->PassVertsOff();triangulated->Update();
        auto* cut=triangulated->GetOutput();if(!cut->GetNumberOfCells())continue;++result.blocks;
        Field p(cut,"p",1),u(cut,"U",3);std::unique_ptr<Field> rho;if(o.useDensityField)rho=std::make_unique<Field>(cut,"rho",1);
        auto* ghosts=cut->GetCellData()->GetArray(vtkDataSetAttributes::GhostArrayName());
        for(vtkIdType i=0;i<cut->GetNumberOfCells();++i) {
            if(ghosts&&ghosts->GetComponent(i,0)!=0){++result.ghostTriangles;continue;}
            auto* cell=cut->GetCell(i);if(cell->GetNumberOfPoints()!=3)continue;
            std::array<Vec3,3> vertices;std::array<std::array<double,3>,3> sorted;
            for(int j=0;j<3;++j){double xyz[3];cut->GetPoint(cell->GetPointId(j),xyz);for(double v:xyz)require(std::isfinite(v),"The section contains non-finite geometry");vertices[j]={xyz[0],xyz[1],xyz[2]};sorted[j]={xyz[0],xyz[1],xyz[2]};}
            std::sort(sorted.begin(),sorted.end());std::array<double,9> key{};for(int j=0;j<3;++j)for(int k=0;k<3;++k)key[j*3+k]=sorted[j][k];
            std::array<double,15> signature{};
            for(int j=0;j<3;++j){std::array<double,3> bary{};for(int k=0;k<3;++k)if(vertices[k].x==sorted[j][0]&&vertices[k].y==sorted[j][1]&&vertices[k].z==sorted[j][2]){bary[k]=1;break;}
                const double r=rho?rho->value(cell,i,0,bary):o.constantDensity;require(r>0,"The section contains a non-positive density");signature[j*5]=r;for(int k=0;k<3;++k)signature[j*5+k+1]=u.value(cell,i,k,bary);signature[j*5+4]=p.value(cell,i,0,bary)*(o.kinematicPressure?r:1);}
            const auto [found,inserted]=triangles.emplace(key,signature);
            if(!inserted){for(int j=0;j<15;++j)require(std::abs(found->second[j]-signature[j])<=1e-9*std::max({1.,std::abs(signature[j]),std::abs(found->second[j])}),"Coincident section faces have different fields; move the plane slightly inside a cell");++result.duplicateTriangles;continue;}
            const double area=.5*(vertices[1]-vertices[0]).cross(vertices[2]-vertices[0]).norm();if(!std::isfinite(area)||area<=0)continue;
            ++result.triangles;areaSum+=area;for(double value:key)hashDouble(value);
            for(const auto& q:quadrature) {
                const double r=rho?rho->value(cell,i,0,q.b):o.constantDensity;require(r>0,"The section contains a non-positive density");
                Vec3 velocity{u.value(cell,i,0,q.b),u.value(cell,i,1,q.b),u.value(cell,i,2,q.b)};
                const double un=velocity[o.axis]*o.normal,pr=p.value(cell,i,0,q.b)*(o.kinematicPressure?r:1),weight=area*q.w;
                require(std::isfinite(pr),"The section contains non-finite field values");
                for(double v:{r,velocity.x,velocity.y,velocity.z,pr})hashDouble(v);
                const double flux=r*un;mass+=weight*flux;forward+=weight*std::max(0.0,flux);reverse+=weight*std::max(0.0,-flux);
                volume+=weight*un;pressure+=weight*pr;density+=weight*r;normalVelocity+=weight*un;
                momentum+=weight*r*un*un;load+=weight*(pr-o.referencePressure);power+=weight*.5*r*velocity.dot(velocity)*un;
            }
        }
    }
    require(areaSum>0,"The plane does not intersect measurable fluid cells; move it inside the volume");
    const auto scaled=[&](long double value){const double number=static_cast<double>(value*result.areaScale);require(std::isfinite(number),"Section integral overflow; review the fields and reference values");return number;};
    result.area=scaled(areaSum);result.massFlow=scaled(mass);result.forwardMassFlow=scaled(forward);result.reverseMassFlow=scaled(reverse);result.volumeFlow=scaled(volume);
    result.meanPressure=static_cast<double>(pressure/areaSum);result.meanDensity=static_cast<double>(density/areaSum);result.meanNormalVelocity=static_cast<double>(normalVelocity/areaSum);
    result.momentumFlux=scaled(momentum);result.pressureLoad=scaled(load);result.resultant=result.momentumFlux+result.pressureLoad;result.kineticPower=scaled(power);
    const double q=.5*o.referenceDensity*o.referenceSpeed*o.referenceSpeed;require(std::isfinite(q)&&q>0&&std::isfinite(q*o.referenceArea)&&q*o.referenceArea>0,"Invalid reference normalization");result.coefficient=result.resultant/(q*o.referenceArea);result.meanCp=(result.meanPressure-o.referencePressure)/q;
    require(std::isfinite(result.coefficient)&&std::isfinite(result.meanCp),"Invalid reference normalization");result.sampleFingerprint=QString::fromLatin1(fingerprint.result().toHex());return result;
}
}
