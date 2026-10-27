#include "vortia/ui/UiIcon.hpp"
#include "vortia/ui/ProjectExplorerPanel.hpp"

#include <QApplication>
#include <QStyle>
#include <QIcon>

namespace vortia {

ProjectExplorerPanel::ProjectExplorerPanel(QWidget* parent) : QTreeWidget(parent) {
    setHeaderHidden(true);
    setMinimumWidth(190);
    setIconSize(QSize(17, 17));
    setUniformRowHeights(true);
    setAnimated(false);
    setRootIsDecorated(false);
    setSelectionMode(QAbstractItemView::SingleSelection);
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

    // Match the actual Vortia engineering ribbon, instead of displaying
    // unrelated OS folder/drive icons in the project tree.
    const auto toolIcon = [](const QString& id, QStyle::StandardPixmap fallback) {
        const auto bundled = uiIcon(QStringLiteral(":/vortia/icons/") + id + QStringLiteral(".svg"));
        return bundled.isNull() ? QApplication::style()->standardIcon(fallback) : bundled;
    };
    addNode(tr("Geometry"), GeometryNode)->setIcon(
        0, toolIcon(QStringLiteral("geometry"), QStyle::SP_DirIcon));
    addNode(tr("Mesh"), MeshNode)->setIcon(
        0, toolIcon(QStringLiteral("mesh"), QStyle::SP_FileDialogDetailedView));
    addNode(tr("Materials"), MaterialsNode)->setIcon(
        0, toolIcon(QStringLiteral("material"), QStyle::SP_DriveHDIcon));
    addNode(tr("Boundary Conditions"), BoundaryConditionsNode)->setIcon(
        0, toolIcon(QStringLiteral("boundary"), QStyle::SP_DirLinkIcon));
    addNode(tr("Calculation"), SolverNode)->setIcon(
        0, toolIcon(QStringLiteral("solver"), QStyle::SP_ComputerIcon));
    resultsItem_ = addNode(tr("Results"), ResultsNode);
    resultsItem_->setIcon(
        0, toolIcon(QStringLiteral("view"), QStyle::SP_DialogApplyButton));

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
