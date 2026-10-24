#include "vortia/visualization/LineSampling.hpp"
#include <vtkCellDataToPointData.h>
#include <vtkCell.h>
#include <vtkCompositeDataSet.h>
#include <vtkCompositeDataIterator.h>
#include <vtkDataSet.h>
#include <vtkDataArray.h>
#include <vtkLineSource.h>
#include <vtkNew.h>
#include <vtkPointData.h>
#include <vtkProbeFilter.h>
#include <vtkSmartPointer.h>
#include <cmath>
#include <algorithm>
#include <stdexcept>
namespace vortia {
namespace {
vtkDataSet* volume(vtkDataObject* data) {
    vtkDataSet* best=nullptr;
    int bestDimension=-1;
    const auto consider=[&](vtkDataSet* candidate) {
        if(!candidate||candidate->GetNumberOfCells()==0)return;
        // OpenFOAM wedge patches can have as many cells as the fluid block.
        // Prefer volume cells independently of traversal order and patch size.
        const int dimension=candidate->GetCell(0)->GetCellDimension();
        if(dimension>bestDimension || (dimension==bestDimension &&
           (!best||candidate->GetNumberOfCells()>best->GetNumberOfCells()))) {
            best=candidate;bestDimension=dimension;
        }
    };
    consider(vtkDataSet::SafeDownCast(data));
    if(auto* composite=vtkCompositeDataSet::SafeDownCast(data)) {
        auto iterator=vtkSmartPointer<vtkCompositeDataIterator>::Take(composite->NewIterator());
        for(iterator->InitTraversal();!iterator->IsDoneWithTraversal();iterator->GoToNextItem())
            consider(vtkDataSet::SafeDownCast(iterator->GetCurrentDataObject()));
    }
    if(!best||best->GetNumberOfCells()==0)throw std::runtime_error("No field data is loaded");
    return best;
}
}
std::array<double,6> fieldBounds(vtkDataObject* data) {
    std::array<double,6> bounds{};volume(data)->GetBounds(bounds.data());return bounds;
}
std::vector<LineSample> sampleFieldLine(vtkDataObject* data,Vec3 start,Vec3 end,int points,const std::string& requested) {
    double length2=0;
    for(int i=0;i<3;++i) {
        if(!std::isfinite(start[i])||!std::isfinite(end[i]))throw std::runtime_error("Invalid sampling line");
        length2+=(end[i]-start[i])*(end[i]-start[i]);
    }
    if(points<2||points>10000||length2<=0)throw std::runtime_error("Invalid sampling line");
    vtkNew<vtkCellDataToPointData> converted;converted->SetInputData(volume(data));converted->Update();
    vtkNew<vtkLineSource> line;line->SetPoint1(start.x,start.y,start.z);line->SetPoint2(end.x,end.y,end.z);line->SetResolution(points-1);
    vtkNew<vtkProbeFilter> probe;probe->SetInputConnection(line->GetOutputPort());probe->SetSourceConnection(converted->GetOutputPort());probe->Update();
    auto* result=probe->GetOutput();
    std::string field=requested;int component=-1;
    if(field=="U"||field=="|U|")field="U";
    else if(field=="Ux"||field=="Uy"||field=="Uz"){component=field[1]=='x'?0:field[1]=='y'?1:2;field="U";}
    auto* values=result->GetPointData()->GetArray(field.c_str());
    auto* mask=result->GetPointData()->GetArray("vtkValidPointMask");
    if(!values||!mask||component>=values->GetNumberOfComponents())throw std::runtime_error("The loaded result does not contain this field");
    std::vector<LineSample> samples;samples.reserve(static_cast<std::size_t>(points));
    for(int i=0;i<points;++i) {
        double xyz[3];result->GetPoint(i,xyz);double value=0;
        if(field=="U"&&component<0) {
            for(int j=0;j<values->GetNumberOfComponents();++j)value+=std::pow(values->GetComponent(i,j),2);
            value=std::sqrt(value);
        } else value=values->GetComponent(i,std::max(0,component));
        samples.push_back({{xyz[0],xyz[1],xyz[2]},std::sqrt(length2)*i/(points-1),value,
            mask->GetComponent(i,0)!=0&&std::isfinite(value)});
    }
    return samples;
}
}
