#include "vortia/visualization/AxisymmetricDisplay.hpp"
#include "vortia/visualization/LineSampling.hpp"
#include <vtkCell.h>
#include <vtkCellData.h>
#include <vtkCellDataToPointData.h>
#include <vtkDataArray.h>
#include <vtkDataSet.h>
#include <vtkDataSetSurfaceFilter.h>
#include <vtkNew.h>
#include <vtkPointData.h>
#include <vtkPoints.h>
#include <vtkPolyData.h>
#include <vtkTriangleFilter.h>
#include <vtkUnstructuredGrid.h>
#include <algorithm>
#include <array>
#include <cmath>
#include <numbers>
#include <stdexcept>
#include <vector>
namespace vortia {
vtkSmartPointer<vtkUnstructuredGrid> revolveAxisymmetricField(vtkDataObject* data) {
    auto* source=fluidDataSet(data);
    vtkSmartPointer<vtkPolyData> meridian;
    if(source->GetCell(0)->GetCellDimension()==3)meridian=sliceField(data,2,0);
    else {
        vtkNew<vtkCellDataToPointData> points;points->SetInputData(source);points->PassCellDataOn();
        vtkNew<vtkDataSetSurfaceFilter> surface;surface->SetInputConnection(points->GetOutputPort());surface->Update();
        meridian=vtkSmartPointer<vtkPolyData>::New();meridian->ShallowCopy(surface->GetOutput());
    }
    vtkNew<vtkTriangleFilter> triangulate;triangulate->SetInputData(meridian);triangulate->PassLinesOff();triangulate->PassVertsOff();triangulate->Update();
    auto* input=triangulate->GetOutput();const auto count=input->GetNumberOfCells(),n=input->GetNumberOfPoints();
    if(count==0||n==0)throw std::runtime_error("No meridional fluid section is available");
    vtkIdType tupleBytes=24;for(int i=0;i<input->GetPointData()->GetNumberOfArrays();++i)if(auto* array=input->GetPointData()->GetArray(i))tupleBytes+=array->GetDataTypeSize()*array->GetNumberOfComponents();
    vtkIdType cellBytes=64;for(int i=0;i<input->GetCellData()->GetNumberOfArrays();++i)if(auto* array=input->GetCellData()->GetArray(i))cellBytes+=array->GetDataTypeSize()*array->GetNumberOfComponents();
    const vtkIdType bytesPerSector=n*(tupleBytes+8)+count*cellBytes;
    int sectors=static_cast<int>(std::min<vtkIdType>({72,750000/count,1500000/n,(256*1024*1024)/std::max<vtkIdType>(1,bytesPerSector)}));sectors-=sectors%4;
    if(sectors<12)throw std::runtime_error("The axisymmetric display exceeds its memory budget; use the original wedge");
    double bounds[6];input->GetBounds(bounds);const double tolerance=std::max(1e-12,std::abs(bounds[3])*1e-10);
    if(bounds[2]<-tolerance||bounds[3]<=tolerance||std::max(std::abs(bounds[4]),std::abs(bounds[5]))>tolerance)
        throw std::runtime_error("Axisymmetric reconstruction needs a meridional XY section with radius Y >= 0");
    auto output=vtkSmartPointer<vtkUnstructuredGrid>::New();vtkNew<vtkPoints> points;points->SetDataTypeToDouble();output->SetPoints(points);
    auto* pd=input->GetPointData();output->GetPointData()->CopyAllocate(pd,n*sectors);output->GetCellData()->CopyAllocate(input->GetCellData(),count*sectors);
    std::vector<vtkIdType> node(static_cast<std::size_t>(n*sectors));
    for(vtkIdType i=0;i<n;++i){
        double p[3];input->GetPoint(i,p);const double radius=std::max(0.0,p[1]);
        for(int sector=0;sector<sectors;++sector){
            const auto index=static_cast<std::size_t>(i*sectors+sector);
            if(radius<=tolerance&&sector){node[index]=node[static_cast<std::size_t>(i*sectors)];continue;}
            const double theta=2*std::numbers::pi*sector/sectors,c=std::cos(theta),s=std::sin(theta);
            const auto id=points->InsertNextPoint(p[0],radius*c,radius*s);node[index]=id;output->GetPointData()->CopyData(pd,i,id);
            for(int a=0;a<pd->GetNumberOfArrays();++a){auto* array=pd->GetArray(a);if(!array||!array->GetName())continue;auto* target=output->GetPointData()->GetArray(array->GetName());const int components=array->GetNumberOfComponents();
                if(components==3){double v[3];array->GetTuple(i,v);target->SetTuple3(id,v[0],c*v[1]-s*v[2],s*v[1]+c*v[2]);}
                else if(components==9){double tensor[9],rotated[9]={};array->GetTuple(i,tensor);const double rotation[3][3]={{1,0,0},{0,c,-s},{0,s,c}};for(int row=0;row<3;++row)for(int col=0;col<3;++col)for(int j=0;j<3;++j)for(int k=0;k<3;++k)rotated[3*row+col]+=rotation[row][j]*tensor[3*j+k]*rotation[col][k];target->SetTuple(id,rotated);}
            }
        }
    }
    for(vtkIdType i=0;i<count;++i){
        auto* triangle=input->GetCell(i);if(triangle->GetNumberOfPoints()!=3)continue;
        std::array<vtkIdType,3> ids{triangle->GetPointId(0),triangle->GetPointId(1),triangle->GetPointId(2)};double a[3],b[3],c[3];input->GetPoint(ids[0],a);input->GetPoint(ids[1],b);input->GetPoint(ids[2],c);
        const double area=(b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]);if(std::abs(area)<1e-24)continue;if(area<0)std::swap(ids[1],ids[2]);
        int axial=0,axisIndex=-1,radialIndex=-1;for(int j=0;j<3;++j){double p[3];input->GetPoint(ids[static_cast<std::size_t>(j)],p);if(p[1]<=tolerance){++axial;axisIndex=j;}else radialIndex=j;}
        for(int sector=0;sector<sectors;++sector){
            const auto at=[&](int j,int next){return node[static_cast<std::size_t>(ids[static_cast<std::size_t>(j)]*sectors+(sector+next)%sectors)];};vtkIdType cell=-1;
            if(axial==0){const vtkIdType vertices[]={at(0,0),at(1,0),at(2,0),at(0,1),at(1,1),at(2,1)};cell=output->InsertNextCell(VTK_WEDGE,6,vertices);}
            else if(axial==1){const int j=(axisIndex+1)%3,k=(axisIndex+2)%3;const vtkIdType vertices[]={at(j,0),at(j,1),at(k,1),at(k,0),at(axisIndex,0)};cell=output->InsertNextCell(VTK_PYRAMID,5,vertices);}
            else if(axial==2){const vtkIdType vertices[]={at((radialIndex+1)%3,0),at((radialIndex+2)%3,0),at(radialIndex,0),at(radialIndex,1)};cell=output->InsertNextCell(VTK_TETRA,4,vertices);}
            if(cell>=0)output->GetCellData()->CopyData(input->GetCellData(),i,cell);
        }
    }
    // Normals belong to the input section surface, not to the reconstructed volume.
    output->GetPointData()->SetNormals(nullptr);
    if(!output->GetNumberOfCells())throw std::runtime_error("The meridional section has no revolvable cells");
    return output;
}
}
