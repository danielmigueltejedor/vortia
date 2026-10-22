#pragma once

#include <QTreeWidget>

namespace vortia {

/// Workflow tree mirroring the CFD pipeline stages. Selecting a node
/// activates the matching editor in the properties panel.
class ProjectExplorerPanel : public QTreeWidget {
    Q_OBJECT

public:
    /// Stable node identifiers (used as item data, independent of language).
    enum NodeId {
        GeometryNode,
        MeshNode,
        MaterialsNode,
        BoundaryConditionsNode,
        SolverNode,
        ResultsNode
    };

    explicit ProjectExplorerPanel(QWidget* parent = nullptr);

    void setResultsSummary(const QString& text);
    void retranslate();

signals:
    void nodeActivated(int nodeId);

private:
    void buildTree();

    QTreeWidgetItem* resultsItem_{nullptr};
};

} // namespace vortia
