// Navigation is controlled exclusively by Qt; VTK is only the renderer.
#include "vortia/visualization/VtkViewport.hpp"

#include <vtkActor.h>
#include <vtkAppendPolyData.h>
#include <vtkAxesActor.h>
#include <vtkCamera.h>
#include <vtkCallbackCommand.h>
#include <vtkCellArray.h>
#include <vtkCellDataToPointData.h>
#include <vtkCompositeDataIterator.h>
#include <vtkCubeSource.h>
#include <vtkDataObject.h>
#include <vtkDataSetSurfaceFilter.h>
#include <vtkGenericOpenGLRenderWindow.h>
#include <vtkInteractorStyleUser.h>
#include <vtkMath.h>
#include <vtkMultiBlockDataSet.h>
#include <vtkNew.h>
#include <vtkOutlineFilter.h>
#include <vtkCellData.h>
#include <vtkDataArray.h>
#include <vtkFloatArray.h>
#include <vtkPointData.h>
#include <vtkPoints.h>
#include <vtkScalarBarActor.h>
#include <vtkLookupTable.h>
#include <vtkTextProperty.h>
#include <vtkPolyData.h>
#include <vtkPolyDataMapper.h>
#include <vtkProperty.h>
#include <vtkRenderer.h>
#include <vtkUnstructuredGrid.h>
#include <vtkXMLMultiBlockDataReader.h>

#include <array>
#include <algorithm>
#include <cmath>
#include <filesystem>
#include <iostream>

#include <QMouseEvent>
#include <QEvent>
#include <QWheelEvent>
#include <numbers>
#include <QEvent>

namespace vortia {

namespace {

// A perceptually ordered blue/teal/yellow scale (no misleading rainbow jumps).
vtkSmartPointer<vtkLookupTable> makeScientificPalette(const double range[2]) {
    constexpr std::array<std::array<double, 3>, 7> stops{{
        {{0.267, 0.005, 0.329}},
        {{0.283, 0.141, 0.458}},
        {{0.254, 0.265, 0.530}},
        {{0.163, 0.471, 0.558}},
        {{0.134, 0.658, 0.518}},
        {{0.478, 0.821, 0.318}},
        {{0.993, 0.906, 0.144}},
    }};
    auto lut = vtkSmartPointer<vtkLookupTable>::New();
    lut->SetNumberOfTableValues(256);
    lut->SetTableRange(range);
    lut->Build();
    for (int i = 0; i < 256; ++i) {
        const double scaled = (static_cast<double>(i) / 255.0) * (stops.size() - 1);
        const auto base = std::min(static_cast<std::size_t>(scaled), stops.size() - 2);
        const double fraction = scaled - static_cast<double>(base);
        const auto color = [&](std::size_t channel) {
            return stops[base][channel] * (1.0 - fraction) +
                   stops[base + 1][channel] * fraction;
        };
        lut->SetTableValue(i, color(0), color(1), color(2), 1.0);
    }
    return lut;
}

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
    setEnableTouchEventProcessing(false); // Avoid duplicate synthesized touch events.

    // Main scene renderer.
    renderer_ = vtkSmartPointer<vtkRenderer>::New();
    renderer_->SetBackground(0.13, 0.14, 0.16);
    // Reserve the right side for the scientific legend; never overlap CFD geometry.
    renderer_->SetViewport(0.0, 0.0, 0.82, 1.0);
    renderWindow->AddRenderer(renderer_);

    // The legend lives in its own non-interactive renderer. This avoids stale
    // OpenGL text/color-bar fragments when Qt+VTK switches arrays repeatedly.
    legendRenderer_ = vtkSmartPointer<vtkRenderer>::New();
    legendRenderer_->SetViewport(0.82, 0.18, 1.0, 1.0);
    legendRenderer_->SetBackground(0.13, 0.14, 0.16);
    legendRenderer_->InteractiveOff();
    renderWindow->AddRenderer(legendRenderer_);

    // One owned legend for the lifetime of the viewport. Recoloring must
    // never stack a new title and color bar on top of the previous one.
    scalarBar_ = vtkSmartPointer<vtkScalarBarActor>::New();
    scalarBar_->SetOrientationToVertical();
    scalarBar_->SetNumberOfLabels(4);
    scalarBar_->SetPosition(0.07, 0.27);
    scalarBar_->SetWidth(0.88);
    scalarBar_->SetHeight(0.48);
    scalarBar_->SetMaximumWidthInPixels(140);
    scalarBar_->SetMaximumHeightInPixels(320);
    scalarBar_->SetUnconstrainedFontSize(false);
    scalarBar_->GetTitleTextProperty()->SetFontSize(12);
    scalarBar_->GetTitleTextProperty()->BoldOff();
    scalarBar_->GetLabelTextProperty()->SetFontSize(10);
    scalarBar_->GetLabelTextProperty()->BoldOff();
    scalarBar_->SetVisibility(false);
    legendRenderer_->AddViewProp(scalarBar_);


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

    // A passive style has NO camera movement on mouse, timer or touch events.
    // Prior trackball style still moved the scene through Qt's native adapter.
    if (auto* iren = renderWindow->GetInteractor()) {
        auto passive = vtkSmartPointer<vtkInteractorStyleUser>::New();
        iren->SetInteractorStyle(passive);
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

void VtkViewport::showEvent(QShowEvent* event) {
    QVTKOpenGLNativeWidget::showEvent(event);
    // Qt may replace the interactor during OpenGL initialization.
    if (auto* rw = renderWindow()) {
        if (auto* interactor = rw->GetInteractor()) {
            if (!vtkInteractorStyleUser::SafeDownCast(interactor->GetInteractorStyle())) {
                auto passive = vtkSmartPointer<vtkInteractorStyleUser>::New();
                interactor->SetInteractorStyle(passive);
            }
        }
    }
}

void VtkViewport::cancelPointerDrag() {
    intentionalPointerDrag_ = false;
    draggedButton_ = Qt::NoButton;
    unsetCursor();
}

void VtkViewport::leaveEvent(QEvent* event) {
    cancelPointerDrag();
    QVTKOpenGLNativeWidget::leaveEvent(event);
}

void VtkViewport::mousePressEvent(QMouseEvent* event) {
    const auto button = event->button();
    if (button == Qt::LeftButton || button == Qt::MiddleButton || button == Qt::RightButton) {
        intentionalPointerDrag_ = true;
        draggedButton_ = button;
        previousPointerPosition_ = event->position();
        setCursor(Qt::ClosedHandCursor);
    }
    event->accept(); // NEVER forward navigation to VTK's adapter.
}

void VtkViewport::mouseReleaseEvent(QMouseEvent* event) {
    if (event->button() == draggedButton_)
        cancelPointerDrag();
    event->accept();
}

void VtkViewport::mouseMoveEvent(QMouseEvent* event) {
    if (!intentionalPointerDrag_ || !(event->buttons() & draggedButton_)) {
        cancelPointerDrag();
        event->accept();
        return;
    }

    const QPointF delta = event->position() - previousPointerPosition_;
    previousPointerPosition_ = event->position();
    auto* camera = renderer_->GetActiveCamera();
    if (camera && !delta.isNull()) {
        if (draggedButton_ == Qt::LeftButton) {
            // Only an explicit left-button drag can orbit.
            const double degreesPerPixel = 180.0 / std::max(1, height());
            camera->Azimuth(-delta.x() * degreesPerPixel);
            camera->Elevation(delta.y() * degreesPerPixel);
            camera->OrthogonalizeViewUp();
        } else {
            double direction[3], viewUp[3], right[3], up[3];
            camera->GetDirectionOfProjection(direction);
            camera->GetViewUp(viewUp);
            vtkMath::Cross(direction, viewUp, right);
            if (vtkMath::Normalize(right) > 1e-12) {
                vtkMath::Cross(right, direction, up);
                vtkMath::Normalize(up);
                const double worldHeight = camera->GetParallelProjection()
                    ? 2.0 * camera->GetParallelScale()
                    : 2.0 * camera->GetDistance() *
                          std::tan(camera->GetViewAngle() * std::numbers::pi / 360.0);
                const double step = worldHeight / std::max(1, height());
                double position[3], focal[3];
                camera->GetPosition(position);
                camera->GetFocalPoint(focal);
                for (int i = 0; i < 3; ++i) {
                    const double shift = (-delta.x() * right[i] + delta.y() * up[i]) * step;
                    position[i] += shift;
                    focal[i] += shift;
                }
                camera->SetPosition(position);
                camera->SetFocalPoint(focal);
            }
        }
        renderer_->ResetCameraClippingRange();
        update();
    }
    event->accept();
}

void VtkViewport::wheelEvent(QWheelEvent* event) {
    const bool precisionScroll = !event->pixelDelta().isNull();
    const auto mods = event->modifiers();
    if (!precisionScroll || (mods & (Qt::ControlModifier | Qt::MetaModifier))) {
        const double steps = precisionScroll
            ? event->pixelDelta().y() / 100.0
            : event->angleDelta().y() / 120.0;
        if (auto* camera = renderer_->GetActiveCamera()) {
            const double factor = std::pow(1.14, std::clamp(steps, -6.0, 6.0));
            if (camera->GetParallelProjection())
                camera->SetParallelScale(std::max(1e-9, camera->GetParallelScale() / factor));
            else
                camera->Dolly(factor);
            renderer_->ResetCameraClippingRange();
            update();
        }
    }
    event->accept();
}

bool VtkViewport::event(QEvent* event) {
    // QVTKOpenGLNativeWidget's event adapter may handle mouse events without
    // calling our mouseMoveEvent override. Intercept BEFORE the adapter so
    // bare hover can never reach VTK's native trackball event pipeline.
    switch (event->type()) {
    case QEvent::MouseButtonPress:
    case QEvent::MouseButtonDblClick:
        mousePressEvent(static_cast<QMouseEvent*>(event));
        return true;
    case QEvent::MouseButtonRelease:
        mouseReleaseEvent(static_cast<QMouseEvent*>(event));
        return true;
    case QEvent::MouseMove:
        mouseMoveEvent(static_cast<QMouseEvent*>(event));
        return true;
    case QEvent::Wheel:
        wheelEvent(static_cast<QWheelEvent*>(event));
        return true;
    case QEvent::NativeGesture:
    case QEvent::TouchBegin:
    case QEvent::TouchUpdate:
    case QEvent::TouchEnd:
        event->accept();
        return true;
    case QEvent::FocusOut:
    case QEvent::WindowDeactivate:
    case QEvent::Hide:
    case QEvent::Leave:
        cancelPointerDrag();
        break;
    default:
        break;
    }
    return QVTKOpenGLNativeWidget::event(event);
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
    currentData_ = nullptr;
    loadedFile_.clear();
    if (scalarBar_) scalarBar_->SetVisibility(false);
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
    geometryActor_->GetProperty()->SetEdgeVisibility(edgesVisible_);
        geometryActor_->GetProperty()->SetColor(0.62, 0.78, 0.9);
        renderer_->AddActor(geometryActor_);
    }
    resetCamera();
}

void VtkViewport::showBoxDomain(const BoxDomain* box) {
    currentData_ = nullptr;
    loadedFile_.clear();
    if (scalarBar_) scalarBar_->SetVisibility(false);
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

void VtkViewport::showDataSet(vtkObject* data, const std::string& scalarField) {
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
    currentData_ = asData;
    currentField_ = scalarField;
    if (scalarBar_) scalarBar_->SetVisibility(false);

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
        auto surface = vtkSmartPointer<vtkDataSetSurfaceFilter>::New();
        // OpenFOAM exports some fields per cell. Convert these to point
        // values for CONTINUOUS DISPLAY only; the solver's original data
        // and discretization remain unchanged.
        if (g->GetPointData()->GetNumberOfArrays() == 0 &&
            g->GetCellData()->GetNumberOfArrays() > 0) {
            auto interpolate = vtkSmartPointer<vtkCellDataToPointData>::New();
            interpolate->SetInputData(g);
            interpolate->PassCellDataOn();
            interpolate->Update();
            surface->SetInputConnection(interpolate->GetOutputPort());
        } else {
            surface->SetInputData(g);
        }
        surface->Update();
        append->AddInputData(surface->GetOutput());
    }
    append->Update();

    auto* poly = append->GetOutput();
    // Special derived fields from U.
    if (currentField_ == "U" || currentField_ == "|U|" || currentField_ == "Ux" ||
        currentField_ == "Uy" || currentField_ == "Uz") {
        vtkDataArray* uarr = poly->GetPointData()->GetArray("U");
        if (uarr != nullptr && uarr->GetNumberOfComponents() >= 3) {
            vtkIdType n = uarr->GetNumberOfTuples();
            auto* derived = vtkFloatArray::New();
            std::string name = currentField_;
            if (name == "U" || name == "|U|")
                name = "Umag";
            derived->SetName(name.c_str());
            derived->SetNumberOfTuples(n);
            for (vtkIdType i = 0; i < n; ++i) {
                double v[3];
                uarr->GetTuple(i, v);
                if (currentField_ == "U" || currentField_ == "|U|")
                    derived->SetTuple1(i, std::sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]));
                else if (currentField_ == "Ux")
                    derived->SetTuple1(i, v[0]);
                else if (currentField_ == "Uy")
                    derived->SetTuple1(i, v[1]);
                else
                    derived->SetTuple1(i, v[2]);
            }
            // Replace existing if present.
            poly->GetPointData()->RemoveArray(name.c_str());
            poly->GetPointData()->AddArray(derived);
            poly->GetPointData()->SetActiveScalars(name.c_str());
            derived->Delete();
        }
    }

    // Choose a scalar field: explicit request first, then preferred list.
    vtkDataArray* scalars = nullptr;
    bool useCellData = false;
    std::string lookupName = currentField_;
    if (lookupName == "U" || lookupName == "|U|")
        lookupName = "Umag";
    if (!lookupName.empty()) {
        scalars = poly->GetPointData()->GetArray(lookupName.c_str());
        if (scalars) {
            poly->GetPointData()->SetActiveScalars(lookupName.c_str());
        } else {
            scalars = poly->GetCellData()->GetArray(lookupName.c_str());
            if (scalars) {
                poly->GetCellData()->SetActiveScalars(lookupName.c_str());
                useCellData = true;
            }
        }
    }
    if (scalars == nullptr && lookupName.empty()) {
        const char* preferred[] = {"p", "T", "U", "Ma", nullptr};
        for (int i = 0; preferred[i] != nullptr; ++i) {
            scalars = poly->GetPointData()->GetArray(preferred[i]);
            if (scalars != nullptr) {
                poly->GetPointData()->SetActiveScalars(preferred[i]);
                break;
            }
        }
    }
    if (scalars == nullptr && lookupName.empty() && poly->GetPointData()->GetNumberOfArrays() > 0) {
        scalars = poly->GetPointData()->GetArray(0);
        if (scalars != nullptr)
            poly->GetPointData()->SetActiveScalars(scalars->GetName());
    }
    if (scalars == nullptr && lookupName.empty()) {
        scalars = poly->GetCellData()->GetArray(0);
        if (scalars != nullptr) {
            poly->GetCellData()->SetActiveScalars(scalars->GetName());
            useCellData = true;
        }
    }
    if (scalars == nullptr && lookupName.empty() && poly->GetPointData()->GetNumberOfArrays() > 0) {
        scalars = poly->GetPointData()->GetArray(0);
        if (scalars != nullptr)
            poly->GetPointData()->SetActiveScalars(scalars->GetName());
    }
    if (scalars == nullptr && lookupName.empty()) {
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
        else
            mapper->SetScalarModeToUsePointData();
        mapper->ScalarVisibilityOn();
        double rng[2];
        scalars->GetRange(rng);  // Range of the SELECTED field, not a stale active array.
        if (rng[0] == rng[1]) {
            const double padding = std::max(1e-9, std::abs(rng[0]) * 1e-6);
            rng[0] -= padding;
            rng[1] += padding;
        }
        auto palette = makeScientificPalette(rng);
        mapper->SetLookupTable(palette);
        mapper->SetScalarRange(rng);
        mapper->UseLookupTableScalarRangeOn();
        mapper->SetColorModeToMapScalars();

        scalarBar_->SetLookupTable(palette);
        const char* label = (lookupName == "Umag") ? "|U| (m/s)" :
                            (scalars->GetName() ? scalars->GetName() : "Field");
        scalarBar_->SetTitle(label);
        scalarBar_->SetVisibility(true);
    } else {
        // An unavailable field should never show a stale legend.
        mapper->ScalarVisibilityOff();
        scalarBar_->SetVisibility(false);
    }
    geometryActor_ = vtkSmartPointer<vtkActor>::New();
    geometryActor_->SetMapper(mapper);
    geometryActor_->GetProperty()->SetEdgeVisibility(edgesVisible_);
    renderer_->AddActor(geometryActor_);
    if (resetViewOnNextData_) {
        // Thin extruded 2D cases are best presented face-on, not at an
        // accidental isometric angle inherited from an empty scene.
        double bounds[6];
        poly->GetBounds(bounds);
        const double dx = bounds[1] - bounds[0];
        const double dy = bounds[3] - bounds[2];
        const double dz = bounds[5] - bounds[4];
        const double cx = (bounds[0] + bounds[1]) * 0.5;
        const double cy = (bounds[2] + bounds[3]) * 0.5;
        const double cz = (bounds[4] + bounds[5]) * 0.5;
        const double maxExtent = std::max({dx, dy, dz});
        if (maxExtent > 0.0 && dz <= 0.1 * std::min(dx, dy)) {
            auto* camera = renderer_->GetActiveCamera();
            camera->ParallelProjectionOn();
            camera->SetFocalPoint(cx, cy, cz);
            camera->SetPosition(cx, cy, cz + 2.0 * maxExtent);
            camera->SetViewUp(0.0, 1.0, 0.0);
        }
        resetCamera();
    } else if (auto* window = renderWindow()) {
        window->Render();
    }
}

void VtkViewport::loadVtmFile(const std::string& path, const std::string& scalarField) {
    if (path.empty() || !std::filesystem::exists(path))
        return;
    try {
        auto reader = vtkSmartPointer<vtkXMLMultiBlockDataReader>::New();
        reader->SetFileName(path.c_str());
        reader->Update();
        if (reader->GetOutput() != nullptr) {
            resetViewOnNextData_ = (path != loadedFile_);
            showDataSet(reader->GetOutput(), scalarField);
            loadedFile_ = path;
        }
    } catch (...) {
        // Swallow; the caller logs the failure.
    }
}

void VtkViewport::showCellEdges(bool enabled) {
    edgesVisible_=enabled;
    if(geometryActor_) {
        geometryActor_->GetProperty()->SetEdgeVisibility(enabled);
        geometryActor_->GetProperty()->SetEdgeColor(0.18,0.25,0.32);
        if(auto* window=renderWindow())window->Render();
    }
}

void VtkViewport::setColorField(const std::string& fieldName) {
    if (currentData_ == nullptr)
        return;
    currentField_ = fieldName;
    resetViewOnNextData_ = false;
    showDataSet(currentData_, fieldName);
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

void VtkViewport::setViewXY() {
    auto* cam = renderer_->GetActiveCamera();
    cam->SetPosition(0, 0, 1);
    cam->SetFocalPoint(0, 0, 0);
    cam->SetViewUp(0, 1, 0);
    cam->SetParallelProjection(1);
    renderer_->ResetCamera();
    if (auto* w = renderWindow()) w->Render();
}
void VtkViewport::setViewXZ() {
    auto* cam = renderer_->GetActiveCamera();
    cam->SetPosition(0, 1, 0);
    cam->SetFocalPoint(0, 0, 0);
    cam->SetViewUp(0, 0, 1);
    renderer_->ResetCamera();
    if (auto* w = renderWindow()) w->Render();
}
void VtkViewport::setViewYZ() {
    auto* cam = renderer_->GetActiveCamera();
    cam->SetPosition(1, 0, 0);
    cam->SetFocalPoint(0, 0, 0);
    cam->SetViewUp(0, 1, 0);
    renderer_->ResetCamera();
    if (auto* w = renderWindow()) w->Render();
}
void VtkViewport::setViewIsometric() {
    auto* cam = renderer_->GetActiveCamera();
    cam->SetPosition(1, 1, 1);
    cam->SetFocalPoint(0, 0, 0);
    cam->SetViewUp(0, 1, 0);
    renderer_->ResetCamera();
    if (auto* w = renderWindow()) w->Render();
}

void VtkViewport::setDarkTheme(bool dark) {
    if (dark) {
        renderer_->SetBackground(0.13, 0.14, 0.16);
        legendRenderer_->SetBackground(0.13, 0.14, 0.16);
        cornerRenderer_->SetBackground(0.13, 0.14, 0.16);
    } else {
        renderer_->SetBackground(0.93, 0.94, 0.96);
        legendRenderer_->SetBackground(0.93, 0.94, 0.96);
        cornerRenderer_->SetBackground(0.93, 0.94, 0.96);
    }
    if (auto* window = renderWindow())
        window->Render();
}

} // namespace vortia
