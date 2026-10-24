#pragma once

#include "vortia/geometry/Sketch2D.hpp"
#include "vortia/geometry/SketchTemplates.hpp"

#include <QDialog>
#include <functional>

class QDoubleSpinBox;
class QLabel;
class QTableWidget;
class QWidget;
class QPushButton;

namespace vortia {

class SketchCanvas2D;

/// Interactive dimensioned 2D sketch editor. Exports actual mm coordinates
/// and explicit line segments as JSON; no CFD-mesh conversion is implied.
class SketchEditorDialog final : public QDialog {
    Q_OBJECT
public:
    explicit SketchEditorDialog(QWidget* parent = nullptr);
    void setSketch(const Sketch2D& shape);
    [[nodiscard]] const Sketch2D& sketch() const noexcept { return sketch_; }
    [[nodiscard]] const NozzleDimensions& nozzleDimensions() const noexcept { return nozzleDims_; }
    void setNozzleDimensions(NozzleDimensions dimensions) { nozzleDims_ = dimensions; }
    bool writeSketchFile(const QString& path) const;
    bool readSketchFile(const QString& path);
    void exportNozzleMesh();
    void exportWedgeMesh();
    void reject() override;

signals:
    void sketchChanged();
    void closeRequested();
private:
    void refresh();
    bool confirmReplacement() const;
    void setShape(const Sketch2D& shape);
    void setSelectedSegment(int index);
    void setSelectedPoint(int index);
    void addPoint();
    void movePoint();
    void split();
    void changeLength();
    void fillet();
    void changeRadius();
    void save();
    void load();
    Sketch2D sketch_;
    NozzleDimensions nozzleDims_;
    SketchCanvas2D* canvas_{nullptr};
    QPushButton* drawButton_{nullptr};
    QTableWidget* table_{nullptr};
    QLabel* status_{nullptr};
    QLabel* selected_{nullptr};
    QDoubleSpinBox* x_{nullptr};
    QDoubleSpinBox* y_{nullptr};
    int selectedSegment_{-1};
    int selectedPoint_{-1};
};

} // namespace vortia
