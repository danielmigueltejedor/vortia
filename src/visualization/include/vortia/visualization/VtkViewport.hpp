#pragma once

#include "vortia/geometry/BoxDomain.hpp"
#include "vortia/geometry/TriangleMesh.hpp"

#include <QVTKOpenGLNativeWidget.h>

#include <vtkDataObject.h>
#include <vtkSmartPointer.h>

#include <string>

class vtkActor;
class vtkOrientationMarkerWidget;
class vtkRenderer;

namespace vortia {

/// Interactive 3D viewport (VTK embedded in Qt).
///
/// MVP capabilities: imported STL surfaces, box-domain outline, trackball
/// camera, orientation axes, theme-aware background. Result fields (p, U)
/// arrive in Phase 2 via foamToVTK conversion.
class VtkViewport : public QVTKOpenGLNativeWidget {
    Q_OBJECT

public:
    explicit VtkViewport(QWidget* parent = nullptr);
    ~VtkViewport() override;

    /// Replaces the imported-geometry actor (nullptr clears it).
    void showTriangleMesh(const TriangleMesh* mesh);
    /// Replaces the computational-domain outline (nullptr clears it).
    void showBoxDomain(const BoxDomain* box);

    /// Displays an arbitrary VTK dataset (e.g. a Fluent CFF multiblock).
    /// Ownership of the dataset is NOT taken; the caller keeps it alive.
    void showDataSet(vtkObject* data);

    void clearScene();
    void resetCamera();

public slots:
    void setDarkTheme(bool dark);

private:
    vtkSmartPointer<vtkRenderer> renderer_;
    vtkSmartPointer<vtkActor> geometryActor_;
    vtkSmartPointer<vtkActor> domainActor_;
    vtkSmartPointer<vtkOrientationMarkerWidget> orientationMarker_;
};

} // namespace vortia
