#include "vortia/visualization/VtkViewport.hpp"

#include <vtkActor.h>
#include <vtkAppendPolyData.h>
#include <vtkAxesActor.h>
#include <vtkCamera.h>
#include <vtkCallbackCommand.h>
#include <vtkCellArray.h>
#include <vtkCompositeDataIterator.h>
#include <vtkCubeSource.h>
#include <vtkDataObject.h>
#include <vtkDataSetSurfaceFilter.h>
#include <vtkGenericOpenGLRenderWindow.h>
#include <vtkInteractorStyleTrackballCamera.h>
#include <vtkMultiBlockDataSet.h>
#include <vtkNew.h>
#include <vtkOutlineFilter.h>
#include <vtkPoints.h>
#include <vtkPolyData.h>
#include <vtkPolyDataMapper.h>
#include <vtkProperty.h>
#include <vtkRenderer.h>
#include <vtkUnstructuredGrid.h>

#include <cmath>

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

    // Main scene renderer.
    renderer_ = vtkSmartPointer<vtkRenderer>::New();
    renderer_->SetBackground(0.13, 0.14, 0.16);
    renderer_->SetViewport(0.0, 0.0, 1.0, 1.0);
    renderWindow->AddRenderer(renderer_);

    // Corner axes overlay: separate renderer with its own camera, so the
    // axes never steal mouse events and never spin on hover.
    cornerRenderer_ = vtkSmartPointer<vtkRenderer>::New();
    cornerRenderer_->SetBackground(0.13, 0.14, 0.16);
    cornerRenderer_->SetViewport(0.82, 0.0, 1.0, 0.18);
    cornerRenderer_->InteractiveOff();
    auto cornerCamera = vtkSmartPointer<vtkCamera>::New();
    cornerCamera->SetPosition(1.5, 1.5, 1.5);
    cornerCamera->SetFocalPoint(0.0, 0.0, 0.0);
    cornerCamera->SetViewUp(0.0, 0.0, 1.0);
    cornerRenderer_->SetActiveCamera(cornerCamera);
    renderWindow->AddRenderer(cornerRenderer_);

    auto axes = vtkSmartPointer<vtkAxesActor>::New();
    axes->SetTotalLength(1.0, 1.0, 1.0);
    axes->SetPickable(0);
    cornerRenderer_->AddActor(axes);

    if (renderWindow->GetInteractor() != nullptr) {
        auto style = vtkSmartPointer<vtkInteractorStyleTrackballCamera>::New();
        renderWindow->GetInteractor()->SetInteractorStyle(style);
    }

    // Keep the corner axes aligned with the main camera orientation.
    if (renderer_->GetActiveCamera() != nullptr) {
        auto cb = vtkSmartPointer<vtkCallbackCommand>::New();
        cb->SetCallback([](vtkObject*, unsigned long, void* clientData, void*) {
            auto* self = static_cast<VtkViewport*>(clientData);
            self->syncCornerCamera();
        });
        cb->SetClientData(this);
        renderer_->GetActiveCamera()->AddObserver(vtkCommand::ModifiedEvent, cb);
    }

    setFocusPolicy(Qt::StrongFocus);
    setMinimumSize(320, 240);
}

VtkViewport::~VtkViewport() = default;

void VtkViewport::syncCornerCamera() {
    if (cornerRenderer_ == nullptr || renderer_ == nullptr)
        return;
    vtkCamera* mainCam = renderer_->GetActiveCamera();
    vtkCamera* cornerCam = cornerRenderer_->GetActiveCamera();
    if (mainCam == nullptr || cornerCam == nullptr)
        return;
    double pos[3], fp[3], vu[3];
    mainCam->GetPosition(pos);
    mainCam->GetFocalPoint(fp);
    mainCam->GetViewUp(vu);
    // Mirror orientation: keep the main camera's view direction.
    double dir[3] = {fp[0] - pos[0], fp[1] - pos[1], fp[2] - pos[2]};
    double len = std::sqrt(dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]);
    if (len < 1e-12)
        return;
    // Place corner camera at a fixed distance along the same direction.
    double dist = 3.0;
    cornerCam->SetPosition(dir[0] / len * dist, dir[1] / len * dist, dir[2] / len * dist);
    cornerCam->SetFocalPoint(0.0, 0.0, 0.0);
    cornerCam->SetViewUp(vu[0], vu[1], vu[2]);
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
    if (geometryActor_) {
        renderer_->RemoveActor(geometryActor_);
        geometryActor_ = nullptr;
    }
    auto* asData = vtkDataObject::SafeDownCast(data);
    if (asData == nullptr)
        return;

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
    syncCornerCamera();
    if (auto* window = renderWindow())
        window->Render();
}

void VtkViewport::setDarkTheme(bool dark) {
    if (dark) {
        renderer_->SetBackground(0.13, 0.14, 0.16);
        cornerRenderer_->SetBackground(0.13, 0.14, 0.16);
    } else {
        renderer_->SetBackground(0.93, 0.94, 0.96);
        cornerRenderer_->SetBackground(0.93, 0.94, 0.96);
    }
    if (auto* window = renderWindow())
        window->Render();
}

} // namespace vortia