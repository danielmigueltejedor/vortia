#include "vortia/ui/ProjectExplorerPanel.hpp"

namespace vortia {

ProjectExplorerPanel::ProjectExplorerPanel(QWidget* parent) : QTreeWidget(parent) {
    setHeaderHidden(true);
    setMinimumWidth(200);
    buildTree();

    connect(this, &QTreeWidget::currentItemChanged, this,
            [this](QTreeWidgetItem* current, QTreeWidgetItem*) {
                if (current != nullptr)
                    emit nodeActivated(current->data(0, Qt::UserRole).toInt());
            });
}

void ProjectExplorerPanel::buildTree() {
    clear();

    auto addNode = [this](const QString& label, NodeId id) {
        auto* item = new QTreeWidgetItem(this, {label});
        item->setData(0, Qt::UserRole, static_cast<int>(id));
        item->setFlags(item->flags() | Qt::ItemIsEnabled | Qt::ItemIsSelectable);
        return item;
    };

    addNode(tr("Geometry"), GeometryNode);
    addNode(tr("Mesh"), MeshNode);
    addNode(tr("Materials"), MaterialsNode);
    addNode(tr("Boundary Conditions"), BoundaryConditionsNode);
    addNode(tr("Solver"), SolverNode);
    resultsItem_ = addNode(tr("Results"), ResultsNode);

    expandAll();
    if (topLevelItemCount() > 0)
        setCurrentItem(topLevelItem(0));
}

void ProjectExplorerPanel::setResultsSummary(const QString& text) {
    if (resultsItem_ != nullptr)
        resultsItem_->setText(0, tr("Results") + (text.isEmpty() ? QString() : " — " + text));
}

void ProjectExplorerPanel::retranslate() {
    const int current = currentItem() != nullptr ? currentItem()->data(0, Qt::UserRole).toInt()
                                                 : static_cast<int>(GeometryNode);
    buildTree();
    for (int i = 0; i < topLevelItemCount(); ++i) {
        if (topLevelItem(i)->data(0, Qt::UserRole).toInt() == current) {
            setCurrentItem(topLevelItem(i));
            break;
        }
    }
}

} // namespace vortia
