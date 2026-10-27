#pragma once

#include "vortia/geometry/Sketch2D.hpp"
#include "vortia/geometry/SketchTemplates.hpp"

#include <QDialog>
#include <functional>
#include <cstdint>

class QAction;
class QDoubleSpinBox;
class QLabel;
class QTableWidget;
class QWidget;

namespace vortia {

class SketchCanvas2D;

/// Interactive dimensioned 2D sketch editor. Exports actual mm coordinates
/// and explicit line segments as JSON; no CFD-mesh conversion is implied.
class SketchEditorDialog final : public QDialog {
    Q_OBJECT
public:
    explicit SketchEditorDialog(QWidget* parent = nullptr);
    struct EditSnapshot {Sketch2D sketch;NozzleDimensions dimensions;};
    struct EditHistory {std::vector<EditSnapshot> undo,redo;std::uint64_t revision=0;};
    const EditHistory& editHistory() const {return history_;}
    void setEditHistory(EditHistory history);
    void undoEdit();void redoEdit();void copySelection();void pasteGeometry();void deleteSelectedPoint();
protected:
    bool eventFilter(QObject*,QEvent*) override;
public:
    void setSketch(const Sketch2D& shape);
    void setInteractionPreferences(bool snap,double spacing,bool grid,bool dimensions,double zoom);
    void setReadOnly(bool readOnly);
    void setEmbedded(bool embedded);
    void setToolPanelVisible(bool visible);
    void loadSketch() { if(!readOnly_)load(); }
    void saveSketch() { save(); }
    [[nodiscard]] bool isReadOnly() const noexcept { return readOnly_; }
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
    void recordEdit();
    void refreshEditActions();
    void applySnapshot(const EditSnapshot&);
    void refresh();
    bool confirmReplacement() const;
    void setShape(const Sketch2D& shape);
    void setSelectedSegment(int index);
    void setSelectedPoint(int index);
    void addPoint();
    void movePoint();
    void editVertexCoordinates(int index);
    void split();
    void changeLength();
    void fillet();
    void changeRadius();
    void save();
    void load();
    Sketch2D sketch_;
    NozzleDimensions nozzleDims_;
    SketchCanvas2D* canvas_{nullptr};
    QTableWidget* table_{nullptr};
    QLabel* status_{nullptr};
    QLabel* selected_{nullptr};
    QDoubleSpinBox* x_{nullptr};
    QDoubleSpinBox* y_{nullptr};
    int selectedSegment_{-1};
    int selectedPoint_{-1};
    bool readOnly_{false};
    EditHistory history_;
    EditSnapshot committed_;
    bool restoring_{false},dragEditing_{false},dragRecorded_{false};
    QAction *undoAction_{nullptr},*redoAction_{nullptr},*copyAction_{nullptr},*pasteAction_{nullptr},*deleteAction_{nullptr};
};

} // namespace vortia
