#include "vortia/visualization/FieldStatistics.hpp"
#include "vortia/visualization/LineSampling.hpp"
#include <vtkCell.h>
#include <vtkCellData.h>
#include <vtkCellSizeFilter.h>
#include <vtkDataArray.h>
#include <vtkDataSet.h>
#include <vtkNew.h>
#include <vtkPointData.h>
#include <algorithm>
#include <cmath>
#include <limits>
#include <stdexcept>
#include <utility>
namespace vortia {
FieldStatistics fieldStatistics(vtkDataObject* data,const std::string& field,int bins) {
    if(bins<1||bins>256)throw std::runtime_error("Use between 1 and 256 histogram bins");
    auto* source=fluidDataSet(data);FieldStatistics result;result.dimension=source->GetCell(0)->GetCellDimension();
    std::string name=field;int component=0;bool magnitude=false;
    if(name=="U"||name=="|U|"){name="U";magnitude=true;}else if(name=="Ux"||name=="Uy"||name=="Uz"){component=name[1]=='x'?0:name[1]=='y'?1:2;name="U";}
    auto* values=source->GetCellData()->GetArray(name.c_str());result.pointAverages=values==nullptr;if(!values)values=source->GetPointData()->GetArray(name.c_str());
    if(!values||values->GetNumberOfComponents()<=component||(name=="U"&&values->GetNumberOfComponents()!=3))throw std::runtime_error("The loaded result does not contain this field");
    if(values->GetNumberOfTuples()<(result.pointAverages?source->GetNumberOfPoints():source->GetNumberOfCells()))throw std::runtime_error("The stored field has incomplete cell or point values");
    vtkNew<vtkCellSizeFilter> sizes;sizes->SetInputData(source);sizes->SetVolumeArrayName("vortiaMeasure3");sizes->SetAreaArrayName("vortiaMeasure2");sizes->SetLengthArrayName("vortiaMeasure1");sizes->Update();
    auto* measured=vtkDataSet::SafeDownCast(sizes->GetOutput());const auto arrayName="vortiaMeasure"+std::to_string(result.dimension);auto* measures=measured?measured->GetCellData()->GetArray(arrayName.c_str()):nullptr;
    if(!measures)throw std::runtime_error("The loaded fluid mesh has no measurable cells");
    const auto value=[&](vtkIdType index){if(!magnitude)return values->GetComponent(index,component);double v[3];values->GetTuple(index,v);return std::hypot(v[0],v[1],v[2]);};
    std::vector<std::pair<double,double>> samples;samples.reserve(static_cast<std::size_t>(source->GetNumberOfCells()));
    auto* ghosts=source->GetCellData()->GetArray(vtkDataSetAttributes::GhostArrayName());
    long double weight=0,mean=0,m2=0;result.minimum=std::numeric_limits<double>::infinity();result.maximum=-result.minimum;
    for(vtkIdType i=0;i<source->GetNumberOfCells();++i){auto* cell=source->GetCell(i);// VTK volumes may be signed for imported cell ordering; reports use geometric measure.
        const double rawMeasure=measures->GetComponent(i,0),measure=std::abs(rawMeasure);double scalar=0;
        const int type=cell->GetCellType();const bool knownVolume=type==VTK_TETRA||type==VTK_HEXAHEDRON||type==VTK_VOXEL||type==VTK_WEDGE||type==VTK_PYRAMID||type==VTK_QUADRATIC_TETRA||type==VTK_QUADRATIC_HEXAHEDRON||type==VTK_TRIQUADRATIC_HEXAHEDRON;
        if(rawMeasure==-1&&!knownVolume){++result.excluded;continue;}
        if(cell->GetCellDimension()!=result.dimension||(ghosts&&ghosts->GetComponent(i,0)!=0)||!std::isfinite(measure)||measure<=0){++result.excluded;continue;}
        if(result.pointAverages){const auto n=cell->GetNumberOfPoints();for(vtkIdType j=0;j<n;++j)scalar+=value(cell->GetPointId(j))/static_cast<double>(n);}else scalar=value(i);
        if(!std::isfinite(scalar)){++result.excluded;continue;}
        const long double next=weight+measure,delta=scalar-mean;mean+=delta*measure/next;m2+=measure*delta*(scalar-mean);weight=next;samples.emplace_back(scalar,measure);result.minimum=std::min(result.minimum,scalar);result.maximum=std::max(result.maximum,scalar);
    }
    if(samples.empty())throw std::runtime_error("No finite measured cells are available for this field");
    result.cells=samples.size();result.measure=static_cast<double>(weight);result.mean=static_cast<double>(mean);result.standardDeviation=std::sqrt(std::max(0.0,static_cast<double>(m2/weight)));result.integral=result.mean*result.measure;
    if(result.minimum==result.maximum)bins=1;const double width=(result.maximum-result.minimum)/bins;result.histogram.resize(static_cast<std::size_t>(bins));
    for(int i=0;i<bins;++i){auto& bin=result.histogram[static_cast<std::size_t>(i)];bin.low=result.minimum+i*width;bin.high=i==bins-1?result.maximum:bin.low+width;}
    for(const auto& [scalar,measure]:samples){const int i=width>0?std::clamp(static_cast<int>((scalar-result.minimum)/width),0,bins-1):0;auto& bin=result.histogram[static_cast<std::size_t>(i)];++bin.cells;bin.measure+=measure;}
    return result;
}
}
