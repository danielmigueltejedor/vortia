#include "vortia/visualization/VtkViewport.hpp"

#include <vtkActor.h>
#include <vtkAppendPolyData.h>
#include <vtkAxesActor.h>
#include <vtkCellArray.h>
#include <vtkCompositeDataIterator.h>
#include <vtkCubeSource.h>
#include <vtkDataObject.h>
#include <vtkDataSetSurfaceFilter.h>
#include <vtkGenericOpenGLRenderWindow.h>
#include <vtkMultiBlockDataSet.h>
#include <vtkNew.h>
#include <vtkOrientationMarkerWidget.h>
#include <vtkOutlineFilter.h>
#include <vtkPoints.h>
#include <vtkPolyData.h>
#include <vtkPolyDataMapper.h>
#include <vtkProperty.h>
#include <vtkRenderer.h>
#include <vtkUnstructuredGrid.h>

namespace vortia {

namespace {

vtkSmartPointer<vtkPolyData> toPolyData(const TriangleMesh& mesh) {
    auto points = vtkSmartPointer<vtkPoints>::New();
    points->SetDataTypeToDouble();
    points->SetNumberOfPoints(static_cast<vtkIdType>(mesh.vertices().size()));
    for (std::size_t i = 0; i < mesh.vertices().size(); ++i) {
        const Vec3& v = mesh.vertices()[i];
        points->SetPoint(static_cast<vtkIdType>(i), v.x, v.y, v.z);
    }
    auto cells = vtkSmartPointer<vtkCellArray>::New();
    for (const auto& t : mesh.triangles()) {
        cells->InsertNextCell(3);
        cells->InsertCellPoint(static_cast<vtkIdType>(t[0]));
        cells->InsertCellPoint(static_cast<vtkIdType>(t[1]));
        cells->InsertCellPoint(static_cast<vtkIdType>(t[2]));
    }
    auto poly = vtkSmartPointer<vtkPolyData>::New();
    poly->SetPoints(points);
    poly->SetPolys(cells);
    return poly;
}

} // namespace

VtkViewport::VtkViewport(QWidget* parent) : QVTKOpenGLNativeWidget(parent) {
    auto renderWindow = vtkSmartPointer<vtkGenericOpenGLRenderWindow>::New();
    setRenderWindow(renderWindow);

    renderer_ = vtkSmartPointer<vtkRenderer>::New();
    renderer_->SetBackground(0.13, 0.14, 0.16); // dark by default
    renderWindow->AddRenderer(renderer_);

    // Orientation marker widget disabled: with VTK 9.7 on macOS it spins
    // continuously on mouse hover (known interactor issue). A fixed
    // world-space axes actor is used instead until a proper overlay is
    // implemented.
    auto axes = vtkSmartPointer<vtkAxesActor>::New();
    axes->SetTotalLength(0.1, 0.1, 0.1);
    axes->SetPickable(0);
    renderer_->AddActor(axes);

    setFocusPolicy(Qt::StrongFocus);
    setMinimumSize(320, 240);
}

VtkViewport::~VtkViewport() {
    if (orientationMarker_)
        orientationMarker_->SetEnabled(0);
}

void VtkViewport::showTriangleMesh(const TriangleMesh* mesh) {
    if (geometryActor_) {
        renderer_->RemoveActor(geometryActor_);
        geometryActor_ = nullptr;
    }
    if (mesh != nullptr && !mesh->isEmpty()) {
        auto mapper = vtkSmartPointer<vtkPolyDataMapper>::New();
        mapper->SetInputData(toPolyData(*mesh));
        mapper->ScalarVisibilityOff();
        geometryActor_ = vtkSmartPointer<vtkActor>::New();
        geometryActor_->SetMapper(mapper);
        geometryActor_->GetProperty()->SetColor(0.62, 0.78, 0.9);
        geometryActor_->GetProperty()->SetEdgeVisibility(0);
        renderer_->AddActor(geometryActor_);
    }
    resetCamera();
}

void VtkViewport::showBoxDomain(const BoxDomain* box) {
    if (domainActor_) {
        renderer_->RemoveActor(domainActor_);
        domainActor_ = nullptr;
    }
    if (box != nullptr && box->isValid()) {
        auto cube = vtkSmartPointer<vtkCubeSource>::New();
        cube->SetBounds(box->min().x, box->max().x, box->min().y, box->max().y, box->min().z,
                        box->max().z);
        auto outline = vtkSmartPointer<vtkOutlineFilter>::New();
        outline->SetInputConnection(cube->GetOutputPort());
        auto mapper = vtkSmartPointer<vtkPolyDataMapper>::New();
        mapper->SetInputConnection(outline->GetOutputPort());
        domainActor_ = vtkSmartPointer<vtkActor>::New();
        domainActor_->SetMapper(mapper);
        domainActor_->GetProperty()->SetColor(0.95, 0.75, 0.2);
        domainActor_->GetProperty()->SetLineWidth(1.5);
        renderer_->AddActor(domainActor_);
    }
    resetCamera();
}

void VtkViewport::showDataSet(vtkObject* data) {
    auto* asData = vtkDataObject::SafeDownCast(data);
    // Remove previous geometry actor.
    if (geometryActor_) {
        renderer_->RemoveActor(geometryActor_);
        geometryActor_ = nullptr;
    }
    if (asData == nullptr)
        return;

    // Collect unstructured grids from a (possibly composite) dataset.
    std::vector<vtkUnstructuredGrid*> grids;
    if (auto* mb = vtkMultiBlockDataSet::SafeDownCast(asData)) {
        auto* it = mb->NewIterator();
        for (it->InitTraversal(); !it->IsDoneWithTraversal(); it->GoToNextItem()) {
            if (auto* g = vtkUnstructuredGrid::SafeDownCast(it->GetCurrentDataObject()))
                grids.push_back(g);
        }
        it->Delete();
    } else if (auto* g = vtkUnstructuredGrid::SafeDownCast(asData)) {
        grids.push_back(g);
    }

    if (grids.empty()) {
        resetCamera();
        return;
    }

    // Render the surface of each grid as a single actor with multiple parts
    // is not trivial; use one actor per grid appended via a vtkAppendPolyData.
    auto append = vtkSmartPointer<vtkAppendPolyData>::New();
    for (auto* g : grids) {
        auto* sf = vtkDataSetSurfaceFilter::New();
        sf->SetInputData(g);
        sf->Update();
        append->AddInputData(sf->GetOutput());
        sf->Delete();
    }
    append->Update();

    auto mapper = vtkSmartPointer<vtkPolyDataMapper>::New();
    mapper->SetInputData(append->GetOutput());
    mapper->ScalarVisibilityOff();
    geometryActor_ = vtkSmartPointer<vtkActor>::New();
    geometryActor_->SetMapper(mapper);
    geometryActor_->GetProperty()->SetColor(0.62, 0.78, 0.9);
    renderer_->AddActor(geometryActor_);
    resetCamera();
}

void VtkViewport::clearScene() {
    showTriangleMesh(nullptr);
    showBoxDomain(nullptr);
}

void VtkViewport::resetCamera() {
    renderer_->ResetCamera();
    if (auto* window = renderWindow())
        window->Render();
}

void VtkViewport::setDarkTheme(bool dark) {
    if (dark)
        renderer_->SetBackground(0.13, 0.14, 0.16);
    else
        renderer_->SetBackground(0.93, 0.94, 0.96);
    if (auto* window = renderWindow())
        window->Render();
}

} // namespace vortia
