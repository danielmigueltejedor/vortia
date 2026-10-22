#pragma once

#include "vortia/physics/Physics.hpp"

#include <QComboBox>
#include <QDoubleSpinBox>
#include <QLabel>
#include <QPushButton>
#include <QSpinBox>
#include <QStackedWidget>
#include <QWidget>

namespace vortia {

/// Contextual editor. One stacked page per workflow node. The panel owns the
/// case setup being edited (a copy) and emits `setupChanged` on Apply.
class PropertiesPanel : public QWidget {
    Q_OBJECT

public:
    explicit PropertiesPanel(QWidget* parent = nullptr);

    void setSetup(const CaseSetup& setup);
    const CaseSetup& setup() const { return setup_; }

    /// Switches the visible editor to the given node id (ProjectExplorerPanel::NodeId).
    void showNode(int nodeId);

signals:
    void setupChanged(const vortia::CaseSetup& setup);
    void validationReported(const QString& text);

private:
    void buildDomainPage();
    void buildMeshPage();
    void buildMaterialPage();
    void buildBoundaryPage();
    void buildSolverPage();
    void buildResultsPage();

    void loadEditorsFromSetup();
    void applyEditorsToSetup();

    CaseSetup setup_{};
    QStackedWidget* stack_{nullptr};
    // Domain editors
    QDoubleSpinBox* dx0_{nullptr}; // min x
    QDoubleSpinBox* dx1_{nullptr}; // max x
    QDoubleSpinBox* dy0_{nullptr};
    QDoubleSpinBox* dy1_{nullptr};
    QDoubleSpinBox* dz0_{nullptr};
    QDoubleSpinBox* dz1_{nullptr};
    // Mesh
    QSpinBox* cellsX_{nullptr};
    QSpinBox* cellsY_{nullptr};
    QSpinBox* cellsZ_{nullptr};
    // Material
    QDoubleSpinBox* nuEdit_{nullptr};
    // Boundary (lid)
    QDoubleSpinBox* lidUx_{nullptr};
    // Solver
    QComboBox* solverCombo_{nullptr};
    QDoubleSpinBox* endTimeEdit_{nullptr};
    QDoubleSpinBox* deltaTEdit_{nullptr};
    QSpinBox* writeIntervalEdit_{nullptr};

    QPushButton* applyButton_{nullptr};
};

} // namespace vortia
