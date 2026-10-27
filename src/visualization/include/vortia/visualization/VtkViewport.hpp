#pragma once

#include "vortia/geometry/BoxDomain.hpp"
#include "vortia/geometry/TriangleMesh.hpp"
#include "vortia/visualization/LineSampling.hpp"
#include "vortia/visualization/FieldStatistics.hpp"

#include <QVTKOpenGLNativeWidget.h>
#include <QPointF>
#include <QImage>

#include <vtkDataObject.h>
#include <vtkSmartPointer.h>

#include <string>

class vtkActor;
class vtkCamera;
class vtkScalarBarActor;
class vtkOrientationMarkerWidget;
class vtkRenderer;
class QTimer;
class QLabel;

namespace vortia {

/// Interactive 3D viewport (VTK embedded in Qt).
///
/// Displays saved CFD fields, sections and imported geometry. Qt owns camera
/// navigation so hover cannot move the scene. Analysis retains source fields.
class VtkViewport : public QVTKOpenGLNativeWidget {
    Q_OBJECT

public:
    enum class NavigationMode { Orbit, Pan, Zoom };
    explicit VtkViewport(QWidget* parent = nullptr);
    ~VtkViewport() override;

    /// Replaces the imported-geometry actor (nullptr clears it).
    void showTriangleMesh(const TriangleMesh* mesh);
    /// Replaces the computational-domain outline (nullptr clears it).
    void showBoxDomain(const BoxDomain* box);

    /// Displays an arbitrary VTK dataset (e.g. a Fluent CFF multiblock).
    /// Retains a VTK reference to the dataset while it is displayed.
    void showDataSet(vtkObject* data, const std::string& scalarField = {});

    /// Loads and displays a .vtm (VTK multiblock) file, e.g. from foamToVTK.
    void loadVtmFile(const std::string& path, const std::string& scalarField = {});

    /// Re-colors the currently loaded dataset by the given field name.
    void setColorField(const std::string& fieldName);
    std::array<double,6> fieldBounds() const {return vortia::fieldBounds(analysisData());}
    std::vector<LineSample> sampleLine(Vec3 start,Vec3 end,int points,const std::string& field) const {
        return sampleFieldLine(analysisData(),start,end,points,field);
    }
    void showCellEdges(bool enabled);
    bool hasDataSet() const {return currentData_!=nullptr;}
    bool axisymmetricAvailable() const {return sourceWedgeAngle_>0;}
    bool axisymmetricDisplay() const {return axisymmetricDisplay_;}
    void setAxisymmetricSource(double wedgeAngle);
    void setAxisymmetricDisplay(bool enabled);
    bool velocityVectors() const {return velocityVectors_;}
    void setVelocityVectors(bool enabled);
    FieldStatistics statistics(const std::string& field) const {return fieldStatistics(currentData_,field);}

    void setNavigationMode(NavigationMode mode);
    void setNavigationPreferences(double zoom,double pan);
    NavigationMode navigationMode() const {return navigationMode_;}
    void setParallelProjection(bool enabled);
    bool parallelProjection() const;
    void setWireframe(bool enabled);
    struct SlicePlaneState {bool enabled;int axis;double position;};
    SlicePlaneState slicePlaneState() const {return {sliceEnabled_,sliceAxis_,slicePosition_};}
    vtkSmartPointer<vtkCamera> copyCamera() const;
    void restoreCamera(vtkCamera* camera);
    void setSlicePlane(int axis,double position);
    void clearSlicePlane();
    bool sliceEnabled() const {return sliceEnabled_;}
    std::vector<std::string> availableFields() const;
    LineSample probePoint(Vec3 point,const std::string& field) const;
    QImage captureImage();

signals:
    void fieldDataChanged();
    void viewToolsChanged();

public:
    void clearScene();
    void resetCamera();
    void setViewXY();
    void setViewXZ();
    void setViewYZ();
    void setViewIsometric();

protected:
    void showEvent(QShowEvent* event) override;
    void leaveEvent(QEvent* event) override;
    void mousePressEvent(QMouseEvent* event) override;
    void mouseReleaseEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void wheelEvent(QWheelEvent* event) override;
    bool event(QEvent* event) override;
    void keyPressEvent(QKeyEvent* event) override;
    void keyReleaseEvent(QKeyEvent* event) override;

private:
    vtkDataObject* analysisData() const;
    void refreshVelocityVectors(vtkDataObject*);
    void updateReconstructionNotice();
    void syncCornerCamera();
    void cancelPointerDrag();
    void panCamera(QPointF pixels);
    void orbitCamera(QPointF pixels);
    void zoomCamera(double factor,QPointF anchor);
    void finishCameraChange();

public slots:
    void setDarkTheme(bool dark);

private:
    vtkSmartPointer<vtkRenderer> renderer_;
    vtkSmartPointer<vtkRenderer> cornerRenderer_;
    vtkSmartPointer<vtkRenderer> legendRenderer_;
    vtkSmartPointer<vtkScalarBarActor> scalarBar_;
    vtkSmartPointer<vtkActor> geometryActor_;
    vtkSmartPointer<vtkActor> domainActor_;
    vtkSmartPointer<vtkDataObject> currentData_;
    vtkSmartPointer<vtkDataObject> sliceData_;
    vtkSmartPointer<vtkDataObject> revolvedData_;
    vtkSmartPointer<vtkActor> vectorsActor_;
    QLabel* reconstructionNotice_;
    QString reconstructionFailure_;
    double sourceWedgeAngle_{0};
    bool axisymmetricDisplay_{false},velocityVectors_{false};
    std::string currentField_;
    std::string loadedFile_;
    bool resetViewOnNextData_{true};
    bool edgesVisible_{false};
    bool wireframe_{false},sliceEnabled_{false},spaceDown_{false},darkTheme_{true};
    double zoomSensitivity_{1},panSensitivity_{1};
    int sliceAxis_{2};double slicePosition_{0};
    NavigationMode navigationMode_{NavigationMode::Orbit};
    QTimer* cameraRenderTimer_;
    // Authoritative Qt press state. Unlike a VTK interactor state, this
    // cannot be set by a stray vtkCommand::MouseMoveEvent.
    bool intentionalPointerDrag_{false};
    Qt::MouseButton draggedButton_{Qt::NoButton};
    QPointF previousPointerPosition_;
};

} // namespace vortia
