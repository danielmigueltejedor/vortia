#include "vortia/visualization/VtkInteractorStyle.hpp"
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
#include <vtkCellData.h>
#include <vtkDataArray.h>
#include <vtkPointData.h>
#include <vtkPoints.h>
#include <vtkScalarBarActor.h>
#include <vtkTextProperty.h>
#include <vtkPolyData.h>
#include <vtkPolyDataMapper.h>
#include <vtkProperty.h>
#include <vtkRenderer.h>
#include <vtkUnstructuredGrid.h>
#include <vtkXMLMultiBlockDataReader.h>

#include <cmath>
#include <filesystem>
#include <iostream>

#include <QMouseEvent>

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
        auto style = vtkSmartPointer<vortia::VtkInteractorStyle>::New();
        renderWindow->GetInteractor()->SetInteractorStyle(style);
    }

    // Corner axes sync disabled for debugging hover-spin. Re-enable once
    // the spin is resolved.
    (void)0;

    setFocusPolicy(Qt::StrongFocus);
    setMinimumSize(320, 240);
}

VtkViewport::~VtkViewport() = default;

void VtkViewport::showEvent(QShowEvent* event) {
    QVTKOpenGLNativeWidget::showEvent(event);
    if (auto* rw = renderWindow()) {
        if (auto* iren = rw->GetInteractor()) {
            if (iren->GetInteractorStyle() == nullptr ||
                dynamic_cast<vortia::VtkInteractorStyle*>(iren->GetInteractorStyle()) == nullptr) {
                auto style = vtkSmartPointer<vortia::VtkInteractorStyle>::New();
                iren->SetInteractorStyle(style);
            }
        }
    }
}

void VtkViewport::leaveEvent(QEvent* event) {
    QVTKOpenGLNativeWidget::leaveEvent(event);
}

void VtkViewport::mouseMoveEvent(QMouseEvent* event) {
    // Only forward mouse motion to VTK when a button is pressed. This
    // prevents hover from triggering any camera movement.
    if (event->buttons() == Qt::NoButton) {
        event->accept();
        return;
    }
    QVTKOpenGLNativeWidget::mouseMoveEvent(event);
}

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
    if (domainActor_) {
        renderer_->RemoveActor(domainActor_);
        domainActor_ = nullptr;
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

    auto* poly = append->GetOutput();
    // Choose a scalar field: prefer pressure, then temperature, then first.
    vtkDataArray* scalars = nullptr;
    bool useCellData = false;
    const char* preferred[] = {"p", "T", "U", "Ma", nullptr};
    for (int i = 0; preferred[i] != nullptr; ++i) {
        scalars = poly->GetPointData()->GetArray(preferred[i]);
        if (scalars != nullptr) {
            poly->GetPointData()->SetActiveScalars(preferred[i]);
            break;
        }
    }
    if (scalars == nullptr && poly->GetPointData()->GetNumberOfArrays() > 0) {
        scalars = poly->GetPointData()->GetArray(0);
        if (scalars != nullptr)
            poly->GetPointData()->SetActiveScalars(scalars->GetName());
    }
    if (scalars == nullptr) {
        scalars = poly->GetCellData()->GetArray(0);
        if (scalars != nullptr) {
            poly->GetCellData()->SetActiveScalars(scalars->GetName());
            useCellData = true;
        }
    }

    auto mapper = vtkSmartPointer<vtkPolyDataMapper>::New();
    mapper->SetInputData(poly);
    if (scalars != nullptr) {
        if (useCellData)
            mapper->SetScalarModeToUseCellData();
        mapper->ScalarVisibilityOn();
        double rng[2];
        poly->GetScalarRange(rng);
        mapper->SetScalarRange(rng);
        mapper->SetColorModeToMapScalars();
        auto* sb = vtkScalarBarActor::New();
        sb->SetLookupTable(mapper->GetLookupTable());
        std::string title = scalars->GetName() ? scalars->GetName() : "field";
        sb->SetTitle(title.c_str());
        sb->SetNumberOfLabels(5);
        // Keep the bar compact and out of the way of the geometry.
        sb->SetWidth(0.08);
        sb->SetHeight(0.4);
        sb->SetPosition(0.90, 0.30);
        vtkTextProperty* tprop = sb->GetTitleTextProperty();
        if (tprop) { tprop->SetFontSize(12); tprop->BoldOff(); }
        vtkTextProperty* lprop = sb->GetLabelTextProperty();
        if (lprop) { lprop->SetFontSize(10); }
        renderer_->AddActor(sb);
        sb->Delete();
        std::cout << "VtkViewport: coloring by '" << (scalars->GetName() ? scalars->GetName() : "?")
                  << "' range=[" << rng[0] << ", " << rng[1] << "] "
                  << (useCellData ? "(cell)" : "(point)") << "\\n";
    } else {
        mapper->ScalarVisibilityOff();
        std::cout << "VtkViewport: no scalar field found\\n";
    }
    geometryActor_ = vtkSmartPointer<vtkActor>::New();
    geometryActor_->SetMapper(mapper);
    renderer_->AddActor(geometryActor_);
    resetCamera();
}

void VtkViewport::loadVtmFile(const std::string& path) {
    if (path.empty() || !std::filesystem::exists(path))
        return;
    try {
        auto reader = vtkSmartPointer<vtkXMLMultiBlockDataReader>::New();
        reader->SetFileName(path.c_str());
        reader->Update();
        if (reader->GetOutput() != nullptr)
            showDataSet(reader->GetOutput());
    } catch (...) {
        // Swallow; the caller logs the failure.
    }
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