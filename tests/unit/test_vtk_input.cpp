#include "vortia/visualization/VtkInteractorStyle.hpp"

#include <gtest/gtest.h>
#include <vtkNew.h>
#include "vortia/visualization/LineSampling.hpp"
#include <vtkImageData.h>
#include <vtkDoubleArray.h>
#include <vtkPointData.h>
#include <vtkMultiBlockDataSet.h>
#include <vtkPlaneSource.h>
#include <vtkPolyData.h>
#include <limits>
#include <algorithm>

TEST(LineSampling, PrefersFluidVolumeOverAnEquallyLargeBoundaryPatch) {
    vtkNew<vtkPlaneSource> patch;patch->SetXResolution(2);patch->SetYResolution(4);
    patch->SetOrigin(0,0,-1);patch->SetPoint1(1,0,-1);patch->SetPoint2(0,1,-1);patch->Update();
    vtkNew<vtkImageData> grid;grid->SetDimensions(3,3,3);grid->SetSpacing(.5,.5,.5);
    ASSERT_EQ(patch->GetOutput()->GetNumberOfCells(),grid->GetNumberOfCells());
    vtkNew<vtkDoubleArray> field;field->SetName("p");field->SetNumberOfTuples(grid->GetNumberOfPoints());
    for(vtkIdType i=0;i<grid->GetNumberOfPoints();++i){double p[3];grid->GetPoint(i,p);field->SetValue(i,p[0]+2*p[1]);}
    grid->GetPointData()->AddArray(field);
    vtkNew<vtkMultiBlockDataSet> blocks;blocks->SetBlock(0,patch->GetOutput());blocks->SetBlock(1,grid);
    const auto bounds=vortia::fieldBounds(blocks);
    EXPECT_DOUBLE_EQ(bounds[4],0);EXPECT_DOUBLE_EQ(bounds[5],1);
    const auto samples=vortia::sampleFieldLine(blocks,{.1,.2,.3},{.9,.2,.3},5,"p");
    for(const auto& sample:samples){EXPECT_TRUE(sample.valid);EXPECT_NEAR(sample.value,sample.position.x+2*sample.position.y,1e-6);}
    blocks->SetBlock(0,grid);blocks->SetBlock(1,patch->GetOutput());
    EXPECT_EQ(vortia::fieldBounds(blocks),bounds);
}

TEST(LineSampling, InterpolatesAnAnalyticalFieldAndPreservesOutsideMasks) {
    vtkNew<vtkImageData> grid;grid->SetDimensions(3,3,3);grid->SetSpacing(.5,.5,.5);
    vtkNew<vtkDoubleArray> field;field->SetName("p");field->SetNumberOfTuples(grid->GetNumberOfPoints());
    for(vtkIdType i=0;i<grid->GetNumberOfPoints();++i){double p[3];grid->GetPoint(i,p);field->SetValue(i,p[0]+2*p[1]);}
    grid->GetPointData()->AddArray(field);
    const auto samples=vortia::sampleFieldLine(grid,{.1,.2,.3},{.9,.2,.3},5,"p");
    ASSERT_EQ(samples.size(),5u);
    for(const auto& sample:samples){EXPECT_TRUE(sample.valid);EXPECT_NEAR(sample.value,sample.position.x+2*sample.position.y,1e-6);}
    const auto crossing=vortia::sampleFieldLine(grid,{-1,.2,.3},{2,.2,.3},4,"p");
    ASSERT_EQ(crossing.size(),4u);EXPECT_FALSE(crossing.front().valid);EXPECT_FALSE(crossing.back().valid);
    EXPECT_TRUE(crossing[1].valid);EXPECT_TRUE(crossing[2].valid);
    EXPECT_THROW(vortia::sampleFieldLine(grid,{0,0,0},{0,0,0},5,"p"),std::runtime_error);
    EXPECT_THROW(vortia::sampleFieldLine(grid,{0,0,0},{1,1,1},5,"missing"),std::runtime_error);
}

TEST(VtkInteractorInput, HoverCannotArmCameraWithoutQtPress) {
    vtkNew<vortia::VtkInteractorStyle> style;
    EXPECT_FALSE(style->qtDragAuthorized());
    style->OnLeftButtonDown();
    style->OnMouseMove();
    EXPECT_FALSE(style->qtDragAuthorized());
}

TEST(VtkInteractorInput, PointerReleaseOrFocusLossDisarmsCamera) {
    vtkNew<vortia::VtkInteractorStyle> style;
    style->setQtDragAuthorized(true);
    EXPECT_TRUE(style->qtDragAuthorized());
    style->setQtDragAuthorized(false);
    style->OnMouseMove();
    EXPECT_FALSE(style->qtDragAuthorized());
}

TEST(VtkInteractorInput, UnsolicitedMiddleOrRightButtonsDoNotArmCamera) {
    vtkNew<vortia::VtkInteractorStyle> style;
    style->OnMiddleButtonDown();
    style->OnRightButtonDown();
    EXPECT_FALSE(style->qtDragAuthorized());
}

TEST(FieldAnalysis, PointProbeAndPlanesPreserveAnalyticalFieldsAndSource) {
    vtkNew<vtkImageData> grid;grid->SetDimensions(3,3,3);grid->SetSpacing(.5,.5,.5);
    vtkNew<vtkDoubleArray> p;p->SetName("p");p->SetNumberOfTuples(grid->GetNumberOfPoints());
    vtkNew<vtkDoubleArray> u;u->SetName("U");u->SetNumberOfComponents(3);u->SetNumberOfTuples(grid->GetNumberOfPoints());
    for(vtkIdType i=0;i<grid->GetNumberOfPoints();++i){double x[3];grid->GetPoint(i,x);p->SetValue(i,x[0]+2*x[1]);u->SetTuple3(i,x[0],2*x[1],3*x[2]);}
    grid->GetPointData()->AddArray(p);grid->GetPointData()->AddArray(u);
    const auto names=vortia::fieldNames(grid);
    EXPECT_EQ(names,(std::vector<std::string>{"Ux","Uy","Uz","p","|U|"}));
    const auto sample=vortia::sampleFieldPoint(grid,{.25,.5,.75},"p");
    ASSERT_TRUE(sample.valid);EXPECT_NEAR(sample.value,1.25,1e-8);
    EXPECT_NEAR(vortia::sampleFieldPoint(grid,{.25,.5,.75},"Uz").value,2.25,1e-8);
    EXPECT_NEAR(vortia::sampleFieldPoint(grid,{.25,.5,.75},"|U|").value,std::sqrt(.25*.25+1+2.25*2.25),1e-8);
    EXPECT_FALSE(vortia::sampleFieldPoint(grid,{-1,.5,.5},"p").valid);
    EXPECT_THROW(vortia::sampleFieldPoint(grid,{0,0,0},"missing"),std::runtime_error);
    EXPECT_THROW(vortia::sampleFieldPoint(grid,{std::numeric_limits<double>::quiet_NaN(),0,0},"p"),std::runtime_error);
    for(int axis=0;axis<3;++axis){
        auto cut=vortia::sliceField(grid,axis,.25);ASSERT_GT(cut->GetNumberOfPoints(),0);
        auto* values=cut->GetPointData()->GetArray("p");ASSERT_NE(values,nullptr);
        for(vtkIdType i=0;i<cut->GetNumberOfPoints();++i){double x[3];cut->GetPoint(i,x);EXPECT_NEAR(x[axis],.25,1e-8);EXPECT_NEAR(values->GetComponent(i,0),x[0]+2*x[1],1e-8);}
    }
    EXPECT_THROW(vortia::sliceField(grid,0,2),std::runtime_error);
    EXPECT_THROW(vortia::sliceField(grid,3,.5),std::runtime_error);
    EXPECT_EQ(grid->GetNumberOfPoints(),27);EXPECT_EQ(grid->GetNumberOfCells(),8);
    EXPECT_EQ(grid->GetPointData()->GetNumberOfArrays(),2);EXPECT_EQ(grid->GetPointData()->GetArray("p"),p.GetPointer());
}
